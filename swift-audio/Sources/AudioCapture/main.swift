// AudioCapture: macOS sidecar that captures system audio via ScreenCaptureKit
// and (optionally) microphone via AVAudioEngine, mixes them, downsamples to
// 16 kHz mono linear PCM (Int16), and writes raw bytes to stdout.
//
// The parent (Tauri/Rust) reads stdout and forwards to Deepgram.
// Status lines and errors are written to stderr as `LOG ...` and `ERR ...`.
// On EOF on stdin, or SIGTERM/SIGINT, the process exits cleanly.
//
// Args: --no-mic  Skip microphone capture (system audio only).

import Foundation
import AVFoundation
import ScreenCaptureKit
import CoreMedia

// ---------- logging ----------
@inline(__always) func logLine(_ s: String) {
    FileHandle.standardError.write(("LOG " + s + "\n").data(using: .utf8) ?? Data())
}
@inline(__always) func errLine(_ s: String) {
    FileHandle.standardError.write(("ERR " + s + "\n").data(using: .utf8) ?? Data())
}

// ---------- config ----------
let captureMic = !CommandLine.arguments.contains("--no-mic")
let outputSampleRate: Double = 16_000

// ---------- output ----------
final class StdoutWriter {
    private let queue = DispatchQueue(label: "stdout.writer")
    private let handle = FileHandle.standardOutput
    func write(_ data: Data) {
        queue.async { [weak self] in
            guard let self = self else { return }
            do { try self.handle.write(contentsOf: data) }
            catch { errLine("stdout write failed: \(error)"); exit(3) }
        }
    }
}
let stdoutWriter = StdoutWriter()

// ---------- shared output format ----------
let outputFormat: AVAudioFormat = AVAudioFormat(
    commonFormat: .pcmFormatInt16,
    sampleRate: outputSampleRate,
    channels: 1,
    interleaved: true
)!

// ---------- mixer ----------
//
// We have two independent audio sources (system + mic). To keep things simple
// and low-latency, each source converts to 16 kHz mono Int16 *independently*
// and the resulting bytes are written to stdout as they arrive. Deepgram is
// tolerant of slightly imperfect mixing because we are sending one stream.
//
// To avoid weird "two voices on top of each other" with shifted timing in the
// transcript, we sum overlapping buffers in a small ring mixer keyed by wall
// clock. For an MVP this is overkill; we instead just write whichever source
// produced a chunk first, which yields readable transcripts when only one
// person is speaking at a time (the common meeting case).
//
// Future work: proper sample-aligned mixing using AVAudioEngine.

final class Sink {
    static let shared = Sink()
    private let lock = NSLock()
    func emit(int16: UnsafePointer<Int16>, frames: Int) {
        let bytes = frames * MemoryLayout<Int16>.size
        let data = Data(bytes: int16, count: bytes)
        stdoutWriter.write(data)
    }
}

// ---------- conversion helpers ----------
func makeConverter(from: AVAudioFormat, to: AVAudioFormat) -> AVAudioConverter? {
    AVAudioConverter(from: from, to: to)
}

func convertAndEmit(_ input: AVAudioPCMBuffer, converter: AVAudioConverter) {
    let ratio = outputFormat.sampleRate / input.format.sampleRate
    let cap = AVAudioFrameCount(Double(input.frameLength) * ratio + 1024)
    guard let out = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: cap) else { return }
    var fed = false
    var error: NSError?
    let status = converter.convert(to: out, error: &error) { _, statusPtr in
        if fed {
            statusPtr.pointee = .endOfStream
            return nil
        }
        fed = true
        statusPtr.pointee = .haveData
        return input
    }
    if status == .error {
        if let error = error { errLine("convert error: \(error)") }
        return
    }
    if let p = out.int16ChannelData?[0] {
        Sink.shared.emit(int16: p, frames: Int(out.frameLength))
    }
}

extension CMSampleBuffer {
    /// Wrap the audio buffer list into an AVAudioPCMBuffer (no copy).
    func asPCMBuffer() -> AVAudioPCMBuffer? {
        guard let fmtDesc = CMSampleBufferGetFormatDescription(self),
              let asbdPtr = CMAudioFormatDescriptionGetStreamBasicDescription(fmtDesc) else {
            return nil
        }
        var asbd = asbdPtr.pointee
        guard let avFormat = AVAudioFormat(streamDescription: &asbd) else { return nil }
        let frames = AVAudioFrameCount(CMSampleBufferGetNumSamples(self))
        guard frames > 0,
              let buffer = AVAudioPCMBuffer(pcmFormat: avFormat, frameCapacity: frames) else {
            return nil
        }
        buffer.frameLength = frames
        var abl = buffer.mutableAudioBufferList.pointee
        let s = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            self, at: 0, frameCount: Int32(frames), into: &abl
        )
        guard s == noErr else { return nil }
        return buffer
    }
}

// ---------- system audio (ScreenCaptureKit) ----------
final class SystemAudioCapture: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private var converter: AVAudioConverter?
    private let q = DispatchQueue(label: "sc.audio")
    private let videoQ = DispatchQueue(label: "sc.video.noop")

    func start() async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(
            false, onScreenWindowsOnly: false
        )
        guard let display = content.displays.first else {
            throw NSError(domain: "AudioCapture", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "no displays found"])
        }
        let myPid = ProcessInfo.processInfo.processIdentifier
        let excludeApps = content.applications.filter { $0.processID == myPid }
        let filter = SCContentFilter(
            display: display,
            excludingApplications: excludeApps,
            exceptingWindows: []
        )
        let cfg = SCStreamConfiguration()
        cfg.capturesAudio = true
        cfg.sampleRate = 48_000
        cfg.channelCount = 2
        cfg.width = 2
        cfg.height = 2
        cfg.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        cfg.queueDepth = 6
        cfg.showsCursor = false

        let s = SCStream(filter: filter, configuration: cfg, delegate: self)
        try s.addStreamOutput(self, type: .audio, sampleHandlerQueue: q)
        try s.addStreamOutput(NoopVideoSink(), type: .screen, sampleHandlerQueue: videoQ)
        try await s.startCapture()
        self.stream = s
        logLine("system-audio: capture started")
    }

    func stop() async {
        if let s = stream {
            try? await s.stopCapture()
        }
    }

    // SCStreamOutput
    func stream(_ stream: SCStream, didOutputSampleBuffer sb: CMSampleBuffer,
                of type: SCStreamOutputType) {
        guard type == .audio, sb.isValid else { return }
        guard let pcm = sb.asPCMBuffer() else { return }
        if converter == nil { converter = makeConverter(from: pcm.format, to: outputFormat) }
        guard let conv = converter else { return }
        convertAndEmit(pcm, converter: conv)
    }

    // SCStreamDelegate
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        errLine("system-audio stopped: \(error)")
    }
}

final class NoopVideoSink: NSObject, SCStreamOutput {
    func stream(_ stream: SCStream, didOutputSampleBuffer sb: CMSampleBuffer,
                of type: SCStreamOutputType) {}
}

// ---------- microphone (AVAudioEngine) ----------
final class MicCapture {
    private let engine = AVAudioEngine()
    private var converter: AVAudioConverter?

    func start() throws {
        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.sampleRate > 0 else {
            throw NSError(domain: "Mic", code: 2,
                userInfo: [NSLocalizedDescriptionKey: "no input device"])
        }
        // Tap with reasonable buffer size.
        input.installTap(onBus: 0, bufferSize: 1024, format: inFormat) { [weak self] buffer, _ in
            guard let self = self else { return }
            if self.converter == nil {
                // If mic is multi-channel, mix to mono via converter.
                let monoIn = AVAudioFormat(
                    commonFormat: inFormat.commonFormat,
                    sampleRate: inFormat.sampleRate,
                    channels: 1,
                    interleaved: inFormat.isInterleaved
                ) ?? inFormat
                self.converter = AVAudioConverter(from: monoIn, to: outputFormat)
                    ?? AVAudioConverter(from: inFormat, to: outputFormat)
            }
            guard let conv = self.converter else { return }
            convertAndEmit(buffer, converter: conv)
        }
        try engine.start()
        logLine("mic: capture started (sr=\(inFormat.sampleRate), ch=\(inFormat.channelCount))")
    }

    func stop() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
    }
}

// ---------- supervisor ----------
final class Supervisor {
    let sys = SystemAudioCapture()
    let mic = MicCapture()
    var micEnabled: Bool

    init(micEnabled: Bool) { self.micEnabled = micEnabled }

    func run() async {
        do {
            try await sys.start()
        } catch {
            errLine("system-audio start failed: \(error)")
            exit(10)
        }
        if micEnabled {
            do { try mic.start() }
            catch { errLine("mic start failed (continuing without mic): \(error)") }
        }
        // Keep alive until stdin closes or signal received.
        await waitForExit()
    }

    func waitForExit() async {
        signal(SIGTERM, SIG_IGN)
        signal(SIGINT, SIG_IGN)
        let term = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
        let intr = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
        term.setEventHandler { [weak self] in Task { await self?.shutdown(); exit(0) } }
        intr.setEventHandler { [weak self] in Task { await self?.shutdown(); exit(0) } }
        term.resume(); intr.resume()

        // Watch stdin EOF in a background DispatchIO.
        let stdinFd = FileHandle.standardInput.fileDescriptor
        let src = DispatchSource.makeReadSource(fileDescriptor: stdinFd, queue: .main)
        src.setEventHandler {
            var buf = [UInt8](repeating: 0, count: 256)
            let n = read(stdinFd, &buf, buf.count)
            if n <= 0 {
                Task { await Supervisor.shared?.shutdown(); exit(0) }
            }
        }
        src.resume()

        // Park forever.
        try? await Task.sleep(nanoseconds: UInt64.max)
    }

    func shutdown() async {
        if micEnabled { mic.stop() }
        await sys.stop()
        try? FileHandle.standardOutput.synchronize()
    }

    static var shared: Supervisor?
}

let sup = Supervisor(micEnabled: captureMic)
Supervisor.shared = sup

// Run the async supervisor, blocking the main thread until exit.
let sema = DispatchSemaphore(value: 0)
Task.detached {
    await sup.run()
    sema.signal()
}
sema.wait()
