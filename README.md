# OneTrueDutchie

Live Dutch→English transcription, translation, summaries, and chat for any meeting on macOS.

The app captures system audio (Teams, Zoom, browser, anything) via macOS
**ScreenCaptureKit**, streams it to **Deepgram Nova-2** for live Dutch
transcription, and uses **Claude Haiku 4.5** for per-segment translation,
running summaries, and a meeting-aware chatbot. No backend — your machine
talks straight to the providers, and meetings are stored as JSON in your app
data directory. API keys live in the macOS Keychain.

## Stack

- **Tauri 2** (Rust + React + TypeScript)
- **Swift sidecar** for ScreenCaptureKit + AVAudioEngine audio capture
- **Deepgram Nova-2** for live STT (~$0.0043/min)
- **Claude Haiku 4.5** for translation, summary, and chat (~$1/MTok in, $5/MTok out)

## Costs at a glance

A 1-hour meeting at typical density costs roughly:

- Deepgram (60 min × $0.0043) ≈ **$0.26**
- Claude translation (~3k input + 3k output tokens) ≈ **$0.02**
- Claude summary (3 refreshes × ~10k input + 1k output) ≈ **$0.05**
- Claude chat (cached transcript, ~1k output per question) ≈ **$0.01/question**

Total for a 1h meeting with light chat use: **~$0.35**.

## Prerequisites

- macOS 13.0 or later
- Xcode command-line tools (`xcode-select --install`)
- Rust (`curl https://sh.rustup.rs -sSf | sh`)
- Node 20+ (`brew install node` or `nvm install 20`)
- A Deepgram API key — https://console.deepgram.com/
- An Anthropic API key — https://console.anthropic.com/

## First-time setup

```bash
git clone <this repo>
cd onetruedutchie

# install JS deps
npm install

# build the Swift audio sidecar (places the binary in src-tauri/binaries/
# with the right -<arch>-apple-darwin suffix Tauri expects)
npm run build:swift
```

## Run in dev mode

```bash
npm run tauri dev
```

The first run will prompt:

1. Open the **Settings** dialog and paste both API keys (stored in Keychain).
2. Click **Start meeting**. macOS will ask for **Screen Recording** and
   **Microphone** permission — grant both, then **stop and restart** the
   meeting (macOS only respects new permissions on the next launch of the
   capture process).

Open Teams (or Zoom, or anything) in another window. Audio from any app on
your Mac, plus your microphone, will appear in the transcript pane in Dutch
and English within a couple of seconds.

## Build a release `.app`

```bash
npm run build:swift
npm run tauri build
# After bundling, patch the Info.plist of the bundled .app so the OS
# permission prompts have proper descriptions:
./scripts/inject-infoplist.sh
```

The bundled app is at `src-tauri/target/release/bundle/macos/OneTrueDutchie.app`.

## How it works

```
                ┌─────────────────────────────┐
                │  Tauri main window (React)  │
                │  TopBar | Transcript |      │
                │  Summary | Chat | History   │
                └──────────────┬──────────────┘
                               │  Tauri events / invoke
                               ▼
   ┌──────────────────────────────────────────────────┐
   │  Rust core (src-tauri/src)                       │
   │   ┌──────────┐  ┌─────────────┐  ┌────────────┐  │
   │   │ commands │  │ orchestrator│  │  Keychain  │  │
   │   └────┬─────┘  └──────┬──────┘  └────────────┘  │
   │        │               │                          │
   │   ┌────▼────┐    ┌─────▼─────┐    ┌──────────┐   │
   │   │ audio   │    │ deepgram  │    │anthropic │   │
   │   │ sidecar │    │   ws      │    │  https   │   │
   │   └────┬────┘    └───────────┘    └──────────┘   │
   └────────┼─────────────────────────────────────────┘
            │ raw 16 kHz mono Int16 PCM (stdout)
            ▼
   ┌─────────────────────────────────────────┐
   │  Swift sidecar (audio-capture-*-darwin) │
   │   ScreenCaptureKit (system audio)       │
   │   AVAudioEngine    (microphone)         │
   │   → AVAudioConverter → Int16 16kHz mono │
   └─────────────────────────────────────────┘
```

- The Swift sidecar emits raw 16 kHz mono Int16 LE PCM on stdout. It logs to
  stderr with `LOG ` / `ERR ` prefixes.
- Rust pipes that PCM into a Deepgram WebSocket session
  (`nova-2`, `nl`, `interim_results=true`, `endpointing=300`,
  `utterance_end_ms=1000`).
- Each finalised utterance becomes a `Segment` (Dutch text). A background
  task asks Claude Haiku to translate it; the English fills in shortly after.
- Every two minutes the orchestrator regenerates a running summary using the
  whole transcript so far. You can force a refresh from the UI.
- The **Ask the meeting** pane streams chat answers from Claude with the
  full transcript provided as a cached prefix, so follow-up questions are
  fast and cheap.
- Meetings are persisted as JSON in
  `~/Library/Application Support/com.onetruedutchie.app/meetings/<uuid>.json`,
  saved every 15 s during a meeting and on stop.

## Common issues

**"audio sidecar not found"** — run `npm run build:swift`.

**No transcript appearing** — open System Settings → Privacy & Security →
Screen Recording (and Microphone) and ensure OneTrueDutchie is enabled. After
toggling, fully quit and relaunch. In dev mode the parent process is `cargo`
or `target/debug/onetruedutchie`, so the prompt may appear under that name.

**"Deepgram closed" error** — likely a bad API key. Re-enter in Settings.

**Translation lags 5+ seconds behind transcript** — translation kicks off
only when an utterance finishes (silence detected). Adjust
`endpointing=300` and `utterance_end_ms=1000` in `src-tauri/src/deepgram.rs`
if your speakers run on without pauses.

**Mic recording is loud / picks up your typing** — pass `--no-mic` from the
sidecar by changing the `include_mic: true` argument in
`src-tauri/src/commands.rs::run_meeting` to `false`.

## Project layout

```
src/                         React + TS frontend
src-tauri/
  src/
    lib.rs                   Tauri app entry
    commands.rs              IPC commands + meeting orchestrator
    audio.rs                 Spawns Swift sidecar, pipes PCM
    deepgram.rs              Live STT WebSocket client
    anthropic.rs             Claude API (translate / summarize / chat stream)
    settings.rs              Keychain-backed API key storage
    storage.rs               Per-meeting JSON file persistence
    state.rs                 Meeting model + shared state
  Info.plist                 NS*UsageDescription strings (dev + bundle)
  tauri.conf.json
  capabilities/default.json
swift-audio/                 ScreenCaptureKit + AVAudioEngine sidecar
scripts/inject-infoplist.sh  Patches bundled .app Info.plist post-build
```
