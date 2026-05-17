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
/// Structured metadata for the parent process — no prefix so Rust can
/// pattern-match the line directly. Used for the audio VU meter etc.
@inline(__always) func metaLine(_ s: String) {
    FileHandle.standardError.write((s + "\n").data(using: .utf8) ?? Data())
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

/// Mixes mic + system audio sample-aligned at the output rate (16 kHz mono
/// Int16) and writes a single coherent stream to stdout.
///
/// Both sources push converted samples here; a 100 ms tick reads up to 1600
/// samples from each ring buffer, zero-pads if either is short, sums them
/// sample-by-sample (clamped to Int16), and writes the result to stdout.
///
/// This is what fixes "everything appears twice in the transcript": the old
/// implementation just appended whatever bytes arrived first, so a phrase
/// captured by both mic and the system loopback was concatenated rather than
/// summed — Deepgram heard it twice.
final class Sink {
    static let shared = Sink()
    private let lock = NSLock()
    private var micBuf: [Int16] = []
    private var sysBuf: [Int16] = []
    /// Cap each buffer at ~500 ms so we don't grow unbounded if the consumer
    /// (this sink's tick) ever falls behind a producer.
    private let maxBuffered = 8000
    private var timer: DispatchSourceTimer?
    private var bytesSinceLog: Int = 0
    private var lastLog: Date = Date()
    /// Counts ticks so we can emit audio-level info every Nth tick instead
    /// of on every 100 ms tick (5/sec is plenty for a UI meter).
    private var ticksSinceMeta: Int = 0

    func start() {
        let t = DispatchSource.makeTimerSource(
            queue: DispatchQueue(label: "sink.mixer", qos: .userInteractive))
        t.schedule(deadline: .now() + .milliseconds(100), repeating: .milliseconds(100))
        t.setEventHandler { [weak self] in self?.tick() }
        t.resume()
        self.timer = t
    }

    func append(int16: UnsafePointer<Int16>, frames: Int, source: String) {
        guard frames > 0 else { return }
        let buf = Array(UnsafeBufferPointer(start: int16, count: frames))
        lock.lock()
        if source == "mic" {
            micBuf.append(contentsOf: buf)
            if micBuf.count > maxBuffered {
                micBuf.removeFirst(micBuf.count - maxBuffered)
            }
        } else {
            sysBuf.append(contentsOf: buf)
            if sysBuf.count > maxBuffered {
                sysBuf.removeFirst(sysBuf.count - maxBuffered)
            }
        }
        lock.unlock()
    }

    private func tick() {
        let frames = 1600 // 100 ms at 16 kHz
        var mic = [Int16](repeating: 0, count: frames)
        var sys = [Int16](repeating: 0, count: frames)
        var micCount = 0
        var sysCount = 0
        lock.lock()
        if !micBuf.isEmpty {
            micCount = min(micBuf.count, frames)
            for i in 0..<micCount { mic[i] = micBuf[i] }
            micBuf.removeFirst(micCount)
        }
        if !sysBuf.isEmpty {
            sysCount = min(sysBuf.count, frames)
            for i in 0..<sysCount { sys[i] = sysBuf[i] }
            sysBuf.removeFirst(sysCount)
        }
        let micDepth = micBuf.count
        let sysDepth = sysBuf.count
        bytesSinceLog += frames * 2
        let now = Date()
        let dt = now.timeIntervalSince(lastLog)
        let shouldLog = dt >= 5.0
        let bytesForLog = bytesSinceLog
        if shouldLog {
            bytesSinceLog = 0
            lastLog = now
        }
        lock.unlock()

        // Mix: sum mic + sys sample-by-sample, clamp to Int16 range.
        var out = [Int16](repeating: 0, count: frames)
        for i in 0..<frames {
            let s = Int32(mic[i]) + Int32(sys[i])
            out[i] = Int16(clamping: s)
        }
        out.withUnsafeBufferPointer { ptr in
            let data = Data(buffer: ptr)
            stdoutWriter.write(data)
        }

        // Emit audio-level metadata for the top-bar meter ~5x/sec.
        ticksSinceMeta += 1
        if ticksSinceMeta >= 2 {
            ticksSinceMeta = 0
            let micRms = rms(mic, count: micCount)
            let sysRms = rms(sys, count: sysCount)
            // Compact JSON on stderr with a known prefix; Rust grep
            // recognises the prefix and converts to an audio:level event.
            metaLine("META:level mic=\(String(format: "%.4f", micRms)) sys=\(String(format: "%.4f", sysRms))")
        }

        if shouldLog {
            logLine("mix: \(bytesForLog) bytes/5s (mic=\(micCount), sys=\(sysCount), backlog mic=\(micDepth) sys=\(sysDepth))")
        }
    }
}

/// RMS amplitude of an Int16 PCM buffer, normalised to 0..1.
func rms(_ samples: [Int16], count: Int) -> Double {
    if count == 0 { return 0.0 }
    var acc: Double = 0
    for i in 0..<count {
        let v = Double(samples[i]) / 32768.0
        acc += v * v
    }
    return (acc / Double(count)).squareRoot()
}

// ---------- conversion helpers ----------
func makeConverter(from: AVAudioFormat, to: AVAudioFormat) -> AVAudioConverter? {
    AVAudioConverter(from: from, to: to)
}

func convertAndEmit(_ input: AVAudioPCMBuffer, converter: AVAudioConverter, source: String) {
    let ratio = outputFormat.sampleRate / input.format.sampleRate
    let cap = AVAudioFrameCount(Double(input.frameLength) * ratio + 1024)
    guard let out = AVAudioPCMBuffer(pcmFormat: outputFormat, frameCapacity: cap) else { return }
    var fed = false
    var error: NSError?
    // CRITICAL: never signal `.endOfStream` for a streaming converter — once
    // it sees that, every subsequent convert() call produces 0 frames. Use
    // `.noDataNow` to mean "no more input this round; flush what you can".
    let status = converter.convert(to: out, error: &error) { _, statusPtr in
        if fed {
            statusPtr.pointee = .noDataNow
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
    // .inputRanDry is normal here and means "I produced what I could; come
    // back next call with more input". Any frames in `out` are valid output.
    if out.frameLength > 0, let p = out.int16ChannelData?[0] {
        Sink.shared.append(int16: p, frames: Int(out.frameLength), source: source)
    }
}

/// Wrap a CMSampleBuffer's audio data as an AVAudioPCMBuffer without copying.
/// The returned PCMBuffer is only valid as long as the provided closure runs;
/// the closure is invoked with the buffer (or nil if extraction failed).
///
/// We do this rather than `CMSampleBufferCopyPCMDataIntoAudioBufferList`,
/// which kept failing with kCMSampleBufferError_RequiredParameterMissing
/// (-12731) because AVAudioPCMBuffer's mutableAudioBufferList is statically
/// sized for one mBuffer slot and SC delivers more.
func withPCMBuffer<R>(of sb: CMSampleBuffer, body: (AVAudioPCMBuffer?) -> R) -> R {
    guard let fmtDesc = CMSampleBufferGetFormatDescription(sb),
          let asbdPtr = CMAudioFormatDescriptionGetStreamBasicDescription(fmtDesc) else {
        return body(nil)
    }
    var asbd = asbdPtr.pointee
    guard let avFormat = AVAudioFormat(streamDescription: &asbd) else {
        return body(nil)
    }

    // Discover the AudioBufferList size needed (variable for stereo non-interleaved).
    var sizeNeeded = 0
    var status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sb,
        bufferListSizeNeededOut: &sizeNeeded,
        bufferListOut: nil,
        bufferListSize: 0,
        blockBufferAllocator: nil,
        blockBufferMemoryAllocator: nil,
        flags: 0,
        blockBufferOut: nil
    )
    guard status == noErr, sizeNeeded > 0 else {
        errLine("CMSampleBuffer ABL sizeNeededOut failed: \(status)")
        return body(nil)
    }

    let raw = UnsafeMutableRawPointer.allocate(byteCount: sizeNeeded, alignment: 16)
    defer { raw.deallocate() }
    let ablPtr = raw.bindMemory(to: AudioBufferList.self, capacity: 1)

    var blockBuffer: CMBlockBuffer?
    status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sb,
        bufferListSizeNeededOut: nil,
        bufferListOut: ablPtr,
        bufferListSize: sizeNeeded,
        blockBufferAllocator: nil,
        blockBufferMemoryAllocator: nil,
        flags: kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
        blockBufferOut: &blockBuffer
    )
    guard status == noErr, blockBuffer != nil else {
        errLine("CMSampleBuffer ABL fetch failed: \(status)")
        return body(nil)
    }

    // No-copy wrap. blockBuffer is retained until this scope ends — keep it
    // alive by referencing it inside the closure call below.
    let pcm = AVAudioPCMBuffer(pcmFormat: avFormat, bufferListNoCopy: ablPtr, deallocator: nil)
    let result = body(pcm)
    // Force blockBuffer to live until after `body` returns.
    _ = blockBuffer
    return result
}

// ---------- system audio (ScreenCaptureKit) ----------
final class SystemAudioCapture: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private var converter: AVAudioConverter?
    private let q = DispatchQueue(label: "sc.audio")
    private let videoQ = DispatchQueue(label: "sc.video.noop")
    private var sampleCount: Int = 0
    private var lastLog: Date = Date()

    func start() async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(
            false, onScreenWindowsOnly: false
        )
        guard let display = content.displays.first else {
            throw NSError(domain: "AudioCapture", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "no displays found"])
        }
        // Empty exclusions → capture audio from every app on this display.
        // Self-audio is excluded via `excludesCurrentProcessAudio` on the
        // config below, which is the correct API for that. Putting our own
        // pid in `excludingApplications` here was preventing audio frames
        // from being delivered at all on macOS 14+.
        let filter = SCContentFilter(
            display: display,
            excludingApplications: [],
            exceptingWindows: []
        )
        let cfg = SCStreamConfiguration()
        cfg.capturesAudio = true
        cfg.excludesCurrentProcessAudio = true
        cfg.sampleRate = 48_000
        // MONO. Stereo non-interleaved breaks the CMSampleBuffer →
        // AVAudioPCMBuffer copy because AVAudioPCMBuffer's mutableAudioBufferList
        // is statically sized for 1 mBuffer slot and the copy fails with
        // kCMSampleBufferError_RequiredParameterMissing (-12731). Voice audio
        // is mono anyway — no quality loss for transcription.
        cfg.channelCount = 1
        // 2x2 was a hack to minimise video work, but several macOS versions
        // refuse to deliver audio frames if the video config is degenerate.
        // 100x100 is small enough to be cheap and big enough to be valid.
        cfg.width = 100
        cfg.height = 100
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
        sampleCount += 1
        let now = Date()
        if now.timeIntervalSince(lastLog) > 5.0 {
            lastLog = now
            logLine("sys audio: received \(sampleCount) sample buffers")
        }
        // No-copy wrap. The conversion to 16 kHz Int16 mono inside
        // convertAndEmit copies the data out, so it's safe for the wrapper
        // to deallocate after this call returns.
        withPCMBuffer(of: sb) { pcm in
            guard let pcm = pcm else { return }
            if converter == nil {
                converter = makeConverter(from: pcm.format, to: outputFormat)
            }
            guard let conv = converter else { return }
            convertAndEmit(pcm, converter: conv, source: "sys")
        }
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
    private var tapCallCount: Int = 0
    private var lastTapLog: Date = Date()

    func start() throws {
        let input = engine.inputNode
        let inFormat = input.outputFormat(forBus: 0)
        guard inFormat.sampleRate > 0 else {
            throw NSError(domain: "Mic", code: 2,
                userInfo: [NSLocalizedDescriptionKey: "no input device"])
        }
        logLine("mic: installing tap with format \(inFormat)")
        input.installTap(onBus: 0, bufferSize: 1024, format: inFormat) { [weak self] buffer, _ in
            guard let self = self else {
                errLine("mic tap: self deallocated")
                return
            }
            self.tapCallCount += 1
            let now = Date()
            if now.timeIntervalSince(self.lastTapLog) > 5.0 {
                self.lastTapLog = now
                logLine("mic tap fired count=\(self.tapCallCount) frames=\(buffer.frameLength)")
            }
            if self.converter == nil {
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
            convertAndEmit(buffer, converter: conv, source: "mic")
        }
        engine.prepare()
        try engine.start()
        logLine("mic: capture started (sr=\(inFormat.sampleRate), ch=\(inFormat.channelCount), running=\(engine.isRunning))")

        // Periodically log engine status to detect silent failure.
        DispatchQueue.global().asyncAfter(deadline: .now() + 3) { [weak self] in
            guard let self = self else { return }
            logLine("mic: 3s check — engine.isRunning=\(self.engine.isRunning), tapCalls=\(self.tapCallCount)")
        }
        DispatchQueue.global().asyncAfter(deadline: .now() + 10) { [weak self] in
            guard let self = self else { return }
            logLine("mic: 10s check — engine.isRunning=\(self.engine.isRunning), tapCalls=\(self.tapCallCount)")
        }
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

// Start the mixer pump — drives stdout output at a steady 16 kHz pace
// regardless of how either source produces data.
Sink.shared.start()

let sup = Supervisor(micEnabled: captureMic)
Supervisor.shared = sup

// Kick off captures on a background task; main thread runs the dispatch loop.
// AVAudioEngine + ScreenCaptureKit deliver buffers via libdispatch; if the
// main thread blocks on a semaphore the dispatch sources never fire and the
// audio tap stops calling back after the first buffer.
Task.detached {
    await sup.run()
}
dispatchMain()
