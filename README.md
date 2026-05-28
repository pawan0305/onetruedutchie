# OneTrueDutchie

**Real-time meeting transcription, translation, and AI assistant for macOS.**

[![Download .dmg](https://img.shields.io/badge/Download-OneTrueDutchie_v0.3.1_aarch64.dmg-0a84ff?style=for-the-badge&logo=apple)](https://github.com/pawan0305/onetruedutchie/releases/download/v0.3.1/OneTrueDutchie_0.3.1_aarch64.dmg)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg?style=for-the-badge)](LICENSE)
[![macOS 13+](https://img.shields.io/badge/macOS-13%2B-lightgrey?style=for-the-badge)](#prerequisites)

Built originally to translate Dutch standups into English. Now handles any
language Deepgram can recognise, and translates / summarises into any
language Claude can write. The name stuck.

- **Transcribe** any meeting (Teams, Zoom, Meet, in-person via mic, podcasts in the browser) in real time
- **Translucent subtitle overlay** that floats above every other window, follows you across macOS Spaces, and is fully click-through
- **Translate** every chunk into your target language as it happens
- **Summarise** the whole meeting on demand
- **Chat with the meeting** — ask Claude questions that use the full transcript as context
- **Speaker diarization** with editable speaker names
- **History** with search, tags, and drag-to-merge (combine recordings that should have been one meeting)
- **Custom vocabulary** to boost recognition of names, jargon, and acronyms
- **Audio level VU + cost meter** so you always know it's listening and what you've spent
- 100% local: API keys live in `~/Library/Application Support`, transcripts are JSON files on disk, nothing else leaves your machine besides the calls to Deepgram and Anthropic.

---

## Quick start

**Download (Apple Silicon Macs):**

→ **[OneTrueDutchie_0.3.1_aarch64.dmg](https://github.com/pawan0305/onetruedutchie/releases/download/v0.3.1/OneTrueDutchie_0.3.1_aarch64.dmg)** (~5.4 MB · macOS 13+ · M1/M2/M3/M4)

Double-click the .dmg to mount it, drag OneTrueDutchie into `/Applications`.

Because the .dmg is ad-hoc-signed (not Apple-notarized — notarization
costs $99/yr in the Apple Developer Program), macOS Gatekeeper will
refuse to open it on first launch. **Right-click the app in Applications
and choose Open**, then confirm. After this first launch it opens
normally.

**Build from source:**

```bash
git clone https://github.com/pawan0305/onetruedutchie.git
cd onetruedutchie
bash scripts/install.sh
```

That single script installs everything (Rust, npm deps, builds the Swift
sidecar, signs the .app with a stable local cert so macOS TCC permissions
stick), and drops `OneTrueDutchie.app` into `/Applications`.

Launch from Spotlight or `open /Applications/OneTrueDutchie.app`.

On first launch:

1. Settings → paste your **Deepgram** and **Anthropic** API keys
2. Pick your **Target language** (default English)
3. Start a meeting → grant Screen Recording + Microphone when prompted
4. ⌘Q and relaunch (macOS only honours new TCC permissions on a fresh process)
5. Start another meeting and you're live

---

## API keys

You need **Deepgram** for transcription. For the LLM (translation,
summary, chat), pick one:

| Component | Provider | Where | Cost per hour |
|-----------|----------|-------|---------------|
| **STT** (required) | Deepgram Nova-3 multi | [console.deepgram.com](https://console.deepgram.com/) — $200 free trial | ≈$0.26 |
| **LLM** option A | Anthropic Claude Haiku 4.5 | [console.anthropic.com](https://console.anthropic.com/) | ≈$0.07 |
| **LLM** option B | OpenAI gpt-4o-mini | [platform.openai.com](https://platform.openai.com/) | ≈$0.05 |
| **LLM** option C | Local model via Ollama / LM Studio / vLLM | localhost | **free** |

Settings → **LLM backend** lets you switch between Anthropic and any
OpenAI-compatible endpoint at any time. Local-model setups have
**zero LLM cost** — your only spend is Deepgram (~€0.30/hr).

Keys are stored in `~/Library/Application Support/com.onetruedutchie.app/keys.json`
(chmod 600) and never leave your machine.

---

## Prerequisites

| Tool | Min version |
|------|------|
| macOS | 13 Ventura |
| Xcode Command Line Tools | any recent (`xcode-select --install`) |
| Rust | stable (auto-installed by `setup.sh`) |
| Node.js | 20+ (`brew install node` or `nvm install 22`) |

---

## Features in detail

### Translucent subtitle overlay
A movie-style subtitle window that's always on top, click-through, and
visible on every macOS Space. Each line gets a continuous rounded
highlight (`box-decoration-break: clone`). When unlocked it shows a
small floating control strip with mode toggle (`OFF / source+target /
target only`), font size, lock, and hide buttons — no alt-tab to the
main window mid-meeting.

### Multi-language
Source language is auto-detected per utterance via Deepgram
`language=multi` on Nova-3. Target language for translation, summary,
and chat is set in Settings — pick from 20 common options or type any
language Claude knows. Default English.

### Drag-to-merge history
If you stopped a recording and started a new one in the middle of the
same in-person meeting, grab the `⋮⋮` handle next to one history row
and drop it onto another. Segments, chat, notes, and tags from both
recordings are combined and re-sorted by timestamp — the merged
transcript reads chronologically.

### Speaker diarization
Deepgram acoustic diarization labels speakers 0/1/2/… Click the label
in the transcript to rename them ("Maria", "Sales lead"). Hidden
automatically for solo meetings where everything is speaker 0.

### Custom vocabulary
Settings → Custom vocabulary. One term per line. Boosts these via
Deepgram Nova-3 `keyterm=` — useful for colleague names, project
codenames, and technical jargon.

### Auto-reconnect
A `tokio::sync::broadcast` channel fans out the live audio so if the
Deepgram WebSocket drops mid-meeting, we transparently reconnect (with
exponential backoff) and the audio sidecar keeps running. A coloured
dot in the top bar shows connection state.

### Cost & audio level meters
Top bar shows running per-meeting cost (Deepgram seconds + Anthropic
tokens parsed from each response's `Usage` block) and two VU bars (mic
+ system audio) so you can see at a glance that audio is actually
flowing.

### Notes pane + collapsible sections
Each meeting has a freeform notes textarea (debounced autosave) for
your own observations alongside the AI-generated content. Any pane
(Transcript / Summary / Chat / Notes) can be collapsed to a vertical
strip so you can give the active one more room.

---

## Architecture

```
                ┌─────────────────────────────────┐
                │   Tauri main window (React)      │
                │   TopBar │ Transcript │ Summary  │
                │   Chat   │ Notes      │ History  │
                └──────────────────┬───────────────┘
                                   │ Tauri events / invoke
                                   ▼
   ┌──────────────────────────────────────────────────┐
   │  Tauri overlay window (React, transparent)       │
   │  Subtitles + controls when unlocked              │
   └──────────────────────────────────────────────────┘
                                   ▲
                                   │
   ┌──────────────────────────────────────────────────┐
   │  Rust core (src-tauri/src/)                      │
   │   commands.rs   ← meeting orchestrator           │
   │   audio.rs      ← spawns Swift sidecar           │
   │   deepgram.rs   ← live STT WebSocket             │
   │   anthropic.rs  ← translate / summarise / chat   │
   │   settings.rs   ← keys.json on disk              │
   │   storage.rs    ← per-meeting JSON files         │
   └──────────────────────┬───────────────────────────┘
                          │ raw 16 kHz mono Int16 PCM via stdout
                          ▼
   ┌─────────────────────────────────────────────────┐
   │  Swift sidecar  (swift-audio/)                   │
   │  ScreenCaptureKit  → all system audio            │
   │  AVAudioEngine     → microphone                  │
   │  AVAudioConverter  → 16 kHz mono Int16 LE        │
   └─────────────────────────────────────────────────┘
```

**Flow for one utterance:**

1. Swift sidecar captures PCM from system audio (ScreenCaptureKit) and
   microphone (AVAudioEngine), mixes them sample-aligned, converts to
   16 kHz mono Int16
2. Rust reads chunks from stdin and broadcasts to one or more consumers
3. Deepgram WebSocket consumer streams audio to Nova-3 with
   `language=multi`, `diarize=true`, `keyterm=<your vocab>`
4. Interim text shows up as `segment:pending` events on the overlay and
   transcript pane
5. On is_final the segment is committed — a background task calls
   Claude Haiku to translate the chunk into your target language
6. Translation arrives 1–2 s later, fills in next to the source text
7. Summary / chat calls send the running transcript with prompt
   caching (`cache_control: ephemeral`) so follow-up calls are cheap

---

## Manual dev workflow

```bash
# JS deps
npm install

# Build the Swift sidecar → src-tauri/binaries/
npm run build:swift

# Dev mode (hot-reload frontend, real Rust backend)
npm run tauri dev
```

To build a distributable .app instead:

```bash
bash scripts/install.sh
```

---

## Permissions (macOS)

| Permission | Why |
|-----------|-----|
| **Screen Recording** | ScreenCaptureKit captures system audio from other apps |
| **Microphone** | Optional mic capture (your own voice) |

Grant via System Settings → Privacy & Security after first launch. TCC
permissions are bound to the binary's cdhash; `scripts/install.sh`
creates a stable self-signed certificate so the same permissions stick
across rebuilds.

---

## Troubleshooting

**No transcript appears after starting**
Check Screen Recording permission. Add it in System Settings → Privacy
& Security → Screen Recording. Then ⌘Q the app and relaunch.

**Transcript appears but no translation**
Your Anthropic key is wrong, has no credit, or `translate` is toggled
off (top bar 🌐 button). Re-check Settings and the toggle.

**Subtitle overlay window is opaque gray**
Make sure you're running from `/Applications/OneTrueDutchie.app`
(properly signed and bundled), not `npm run tauri dev`. Transparent
overlays require `macOSPrivateApi: true` + the `macos-private-api`
Cargo feature, both of which the bundled .app has.

**"Deepgram connection closed" error**
Deepgram key is invalid or out of credit. Re-enter in Settings.

**The app asks for permissions, I grant them, but it still doesn't capture**
Stop the meeting, ⌘Q the app, relaunch, start a new meeting. macOS
only honours new TCC permissions on a fresh process.

---

## Project layout

```
onetruedutchie/
├── src/                         React + TypeScript frontend
│   ├── App.tsx                  Root + Tauri event subscriptions
│   ├── styles.css               Dark theme
│   ├── lib/
│   │   ├── tauri.ts             Typed invoke() + listen() wrappers
│   │   └── types.ts             Shared TS interfaces
│   ├── components/
│   │   ├── TopBar.tsx           Start/Stop, dg-status, VU, cost
│   │   ├── TranscriptPane.tsx   Live transcript + speaker labels
│   │   ├── SummaryPane.tsx
│   │   ├── ChatPane.tsx         Streaming chat
│   │   ├── NotesPane.tsx        Debounced notes textarea
│   │   ├── SettingsModal.tsx    Keys, vocab, target language
│   │   ├── HistoryDrawer.tsx    Past meetings + drag-to-merge
│   │   └── Splitter.tsx         Resizable pane divider
│   └── overlay/
│       ├── Overlay.tsx          Subtitle overlay + control strip
│       └── overlay.css
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs               Tauri builder
│   │   ├── commands.rs          IPC commands + meeting orchestrator
│   │   ├── audio.rs             Swift sidecar process + VU events
│   │   ├── deepgram.rs          Deepgram WebSocket client
│   │   ├── anthropic.rs         Claude Haiku translate/summary/chat
│   │   ├── settings.rs          keys.json on disk
│   │   ├── storage.rs           Per-meeting JSON file persistence
│   │   └── state.rs             Meeting / Segment / Chat / Cost model
│   ├── Info.plist               NSScreenCaptureUsageDescription etc.
│   ├── tauri.conf.json          Two windows: main + overlay
│   └── capabilities/default.json
├── swift-audio/
│   ├── Package.swift
│   ├── build.sh                 Builds sidecar → src-tauri/binaries/
│   └── Sources/AudioCapture/main.swift
└── scripts/
    ├── install.sh               One-shot build + sign + install
    ├── setup.sh                 Install Rust + JS deps
    ├── setup-cert.sh            Create stable self-signed cert
    └── inject-infoplist.sh      Patch Info.plist post-build
```

---

## Contributing

PRs welcome. The fastest way to get oriented is:

1. `bash scripts/install.sh` — verify your build works
2. `npm run tauri dev` — hot-reload dev mode
3. Look at `src-tauri/src/commands.rs::run_meeting` for the main loop

If you want to add support for a new feature (new transcription
provider, different overlay style, etc.), open an issue first so we
can talk through the design.

---

## License

MIT — see [LICENSE](LICENSE).

The name "OneTrueDutchie" is just a name — the project translates from
and to any language, not just Dutch.
