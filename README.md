# OneTrueDutchie

**Dutch → English live transcription, translation, and meeting assistant for macOS.**

Captures system audio from any app (Teams, Zoom, browser) via ScreenCaptureKit.
Streams it to Deepgram Nova-2 for live Dutch STT, then uses Claude Haiku to
translate each utterance, refresh a running summary every two minutes, and answer
questions about the meeting via a chat panel.

No backend. API keys stay in your macOS Keychain. Meetings are stored locally.

---

## Quick start

```bash
# 1. Clone
git clone https://github.com/pawan0305/onetruedutchie.git
cd onetruedutchie

# 2. One command — sets up on first run, then starts the app
npm start
```

When the app opens:
- Paste your **Deepgram** and **Anthropic** API keys in Settings.
- Click **Start meeting**. Approve the Screen Recording + Microphone prompts.
- Stop and restart the meeting once (macOS needs a fresh capture process after granting permissions).
- Open Teams / Zoom in another window. Dutch speech shows up as NL + EN text in a couple of seconds.

---

## API keys you need

| Key | Where to get it | Cost for a 1h meeting |
|-----|----------------|----------------------|
| **Deepgram** | [console.deepgram.com](https://console.deepgram.com/) — free tier includes $200 credit | ~$0.26 (Nova-2, streaming) |
| **Anthropic** | [console.anthropic.com](https://console.anthropic.com/) — pay as you go | ~$0.07 (Haiku 4.5, incl. caching) |

Total for a 1-hour meeting with light chat: **≈$0.35**.

---

## Prerequisites

| Tool | Minimum | How to install |
|------|---------|----------------|
| macOS | 13 Ventura | — |
| Xcode CLT | any recent | `xcode-select --install` |
| Rust | stable | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20 | `brew install node` or `nvm install 22` |

`bash scripts/setup.sh` checks for and installs Rust automatically. Node.js must be present first.

---

## Manual step-by-step

```bash
# Install JS dependencies
npm install

# Build the Swift sidecar (writes to src-tauri/binaries/)
npm run build:swift

# Dev mode (hot-reload frontend, real Rust backend)
npm run tauri dev
```

---

## Build a distributable .app

```bash
npm run build:swift
npm run tauri build

# Patch macOS permission descriptions into the bundled .app
bash scripts/inject-infoplist.sh
```

Output: `src-tauri/target/release/bundle/macos/OneTrueDutchie.app`

---

## What's inside

```
                ┌─────────────────────────────────┐
                │   Tauri window (React + TS)      │
                │   TopBar │ Transcript │ Summary  │
                │   Chat   │ History               │
                └──────────────────┬───────────────┘
                                   │ Tauri events / invoke
                                   ▼
   ┌──────────────────────────────────────────────────┐
   │  Rust core (src-tauri/src/)                      │
   │   commands.rs   ← meeting orchestrator           │
   │   audio.rs      ← spawns Swift sidecar           │
   │   deepgram.rs   ← live STT WebSocket             │
   │   anthropic.rs  ← translate / summarise / chat   │
   │   settings.rs   ← Keychain storage               │
   │   storage.rs    ← per-meeting JSON files          │
   └──────────────────────┬───────────────────────────┘
                          │ raw 16 kHz mono Int16 PCM via stdout
                          ▼
   ┌─────────────────────────────────────────────────┐
   │  Swift sidecar  (swift-audio/)                  │
   │  ScreenCaptureKit  → all system audio            │
   │  AVAudioEngine     → microphone                 │
   │  AVAudioConverter  → 16 kHz mono Int16 LE       │
   └─────────────────────────────────────────────────┘
```

**Flow for one Dutch utterance:**

1. Swift sidecar captures PCM → Rust reads stdout chunks
2. Rust streams bytes over WebSocket to **Deepgram Nova-2** (`nl`, `nova-2`, `interim_results=true`)
3. Deepgram emits interim results → UI shows live Dutch text (pending)
4. On `speech_final` → segment is committed → background task calls **Claude Haiku** to translate
5. English text fills in alongside Dutch, usually within 1–2 s
6. Every 2 minutes → Claude re-summarises the full transcript
7. Chat pane streams answers with the transcript as a cached prefix (cheap follow-ups)

---

## Permissions (macOS)

The app needs two TCC permissions:

| Permission | Why |
|-----------|-----|
| **Screen Recording** | ScreenCaptureKit uses this to capture system audio from other apps |
| **Microphone** | Optional mic capture (your own voice into Teams etc.) |

Grant them via **System Settings → Privacy & Security** after first launch. If the permission prompt appears under a different name (`cargo`, `onetruedutchie`) — that's the dev binary. Grant it there.

After granting, stop and restart the meeting once — macOS only honours new permissions on the next launch of the capture process.

---

## UI tour

| Pane | What it does |
|------|-------------|
| **Transcript** | Dutch (orange) + English (blue) side by side. Interim text shows live in faded style; turns solid when finalised. |
| **Summary** | Auto-refreshes every 2 minutes. Hit ↻ to force a refresh. Uses bullets for Decisions / Action items / Open questions. |
| **Ask the meeting** | Chat with Claude about what was discussed. The full transcript is sent as a prompt-cached prefix so follow-up questions are fast. |
| **History** | List of all saved meetings. Click to load a past meeting and chat with it even while a new one is recording. |

---

## Tuning

| Thing to change | Where |
|----------------|-------|
| Summary refresh interval | `src-tauri/src/commands.rs` → `tokio::time::interval(Duration::from_secs(120))` |
| Deepgram endpointing (silence threshold) | `src-tauri/src/deepgram.rs` → `endpointing=300` and `utterance_end_ms=1000` |
| Disable microphone capture | `src-tauri/src/commands.rs` → `audio::start_capture(..., include_mic: false)` |
| Use Sonnet instead of Haiku | `src-tauri/src/anthropic.rs` → change `MODEL_HAIKU` |

---

## Troubleshooting

**"audio sidecar not found"**
→ Run `npm run build:swift`. The Swift binary must exist in `src-tauri/binaries/` before running the app.

**No transcript appears after starting**
→ Check Screen Recording permission. If missing, add it in System Settings → Privacy & Security → Screen Recording. Then fully quit the app and relaunch.

**Transcript appears but no English translation**
→ Your Anthropic key is wrong or has no credit. Re-enter it in Settings.

**Translation lags far behind**
→ Lower the `endpointing` value in `deepgram.rs` (e.g. `200` ms instead of `300`) so utterances finalise sooner.

**"Deepgram connection closed" error**
→ Deepgram key is invalid. Re-enter in Settings.

**The app asks for permissions, I grant them, but it still doesn't capture**
→ Stop the meeting, quit the app fully (`⌘Q`), relaunch, and start a new meeting.

---

## Project layout

```
onetruedutchie/
├── src/                         React + TypeScript frontend
│   ├── App.tsx                  Root component + Tauri event subscriptions
│   ├── styles.css               Dark theme
│   ├── lib/
│   │   ├── tauri.ts             Typed invoke() + listen() wrappers
│   │   └── types.ts             Shared TS interfaces
│   └── components/
│       ├── TopBar.tsx           Start/Stop, title editor, settings button
│       ├── TranscriptPane.tsx   Live Dutch + English segments
│       ├── SummaryPane.tsx      Auto-refreshing summary
│       ├── ChatPane.tsx         Streaming chat Q&A
│       ├── SettingsModal.tsx    API key entry
│       └── HistoryDrawer.tsx    Past meetings list
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs               Tauri builder + plugin setup
│   │   ├── commands.rs          IPC commands + meeting orchestrator
│   │   ├── audio.rs             Spawn Swift sidecar, pipe PCM chunks
│   │   ├── deepgram.rs          Deepgram WebSocket streaming client
│   │   ├── anthropic.rs         Claude API: translate / summarise / chat SSE
│   │   ├── settings.rs          Keychain-backed API key storage
│   │   ├── storage.rs           Per-meeting JSON file persistence
│   │   └── state.rs             Meeting / Segment / ChatMessage model
│   ├── Info.plist               NS*UsageDescription strings
│   ├── tauri.conf.json
│   └── capabilities/
│       └── default.json
├── swift-audio/
│   ├── Package.swift
│   ├── build.sh                 Compiles sidecar → src-tauri/binaries/
│   └── Sources/AudioCapture/
│       └── main.swift           ScreenCaptureKit + AVAudioEngine sidecar
└── scripts/
    ├── setup.sh                 One-shot first-time setup
    └── inject-infoplist.sh      Patches .app bundle Info.plist post-build
```
