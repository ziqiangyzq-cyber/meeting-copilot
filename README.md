# Meeting Copilot

A macOS-native real-time meeting AI assistant: bidirectional Chinese-English live transcription, RAG-aware suggestion stream during the meeting, and structured Markdown / DOCX minutes after.

> Built for technical meetings — design reviews, cross-discipline coordination, field discussions — but works for any meeting scenario.

## Features

- 🎙️ **Dual-source live capture** — system audio (ScreenCaptureKit, no virtual audio driver) + microphone (AVAudioEngine) with macOS-native echo cancellation, noise suppression, and AGC
- 🌐 **Real-time bilingual transcription** — Alibaba Paraformer-realtime-v2 streams Chinese + English with code-switching
- 💡 **In-meeting AI suggestions** — every 20s the LLM reads recent transcript + your uploaded reference materials and surfaces what's worth knowing (standards / risks / open questions / action items)
- 📚 **RAG from pre-meeting materials** — drop a folder of PDFs/docx/md/txt; semantic chunks indexed locally with sqlite-vec
- ⚡ **Live-translate English to Chinese** under each English transcript line
- 📝 **Quick notes during meeting** — typed anchors that drive the post-meeting minutes
- 📋 **3 built-in meeting templates** — Technical Review / Cross-Discipline Coordination / Field Discussion, each with its own minutes schema
- 📄 **Structured minutes** — auto-generated as Markdown + DOCX export
- 🔌 **Pluggable LLM provider** — defaults to MiniMax, or point to any OpenAI-compatible endpoint (DeepSeek, Qwen, OpenAI, Ollama, Groq, etc.)
- 🔒 **No audio recorded** — PCM is streamed directly to ASR and discarded; only text persists
- 🗂️ **Local-only data** — SQLite + local JSON keys file, no cloud sync

## Architecture

```
┌──────────────────────┐
│  Frontend (React)    │  Setup / MeetingView / MinutesView / History / Settings
└──────────┬───────────┘
           │ Tauri IPC
┌──────────▼───────────┐
│  Tauri Main (Rust)   │  ASR / RAG / LLM orchestration · SQLite + sqlite-vec
└──────────┬───────────┘
           │ stdio (PCM frames + JSON commands)
┌──────────▼───────────┐
│  AudioHelper (Swift) │  ScreenCaptureKit (system audio) + AVAudioEngine (mic)
└──────────────────────┘
```

External APIs:
- **Alibaba DashScope** — Paraformer-realtime-v2 (streaming ASR) + text-embedding-v3 (RAG vectors)
- **MiniMax / OpenAI-compat** — chat completions for suggestions, minutes, translation

## Install (macOS)

### Pre-built (recommended for end users)

> A signed installer isn't available yet. Build from source for now, or grab the latest `.dmg` from this repo's Releases page (once published) and follow the right-click → Open path the first time (Gatekeeper).

### Build from source

Requirements: macOS 13+, Xcode CLI tools, Rust 1.75+, Node 22+, pnpm 11+.

```bash
git clone https://github.com/ziqiangyzq-cyber/meeting-copilot.git
cd meeting-copilot
pnpm install
cd audio-helper && swift build -c release && cd ..
pnpm tauri dev    # development mode
# or
pnpm tauri build  # produce src-tauri/target/release/bundle/dmg/Meeting Copilot_*.dmg
```

## First Launch

1. Open the app. The first-launch wizard asks for two API keys:
   - **Alibaba DashScope** — sign up at <https://bailian.console.aliyun.com>, enable `paraformer-realtime-v2` and `text-embedding-v3`
   - **LLM provider** — either a MiniMax key (enable `MiniMax-M2.7-highspeed`) or any OpenAI-compatible base URL + model + key
2. Each input has a "Test" button — verify the key works before saving
3. Grant macOS permissions when prompted: Screen Recording (for system audio) + Microphone
4. Restart the app after granting permissions
5. Done

Keys are stored locally at `~/Library/Application Support/com.efc.meeting-copilot/keys.json` (owner-only, `0600`). No cloud upload.

> The `com.efc.` namespace in the bundle identifier is historical naming — it does not imply any organizational affiliation.

## Windows

Source includes a `audio-helper-win/` Rust + WASAPI scaffold but it has not been compiled or tested on a real Windows machine yet. See `audio-helper-win/README.md`.

## Project Structure

```
meeting-copilot/
├── audio-helper/         # Swift CLI for macOS audio capture
├── audio-helper-win/     # Rust + WASAPI scaffold for Windows (untested)
├── src-tauri/            # Rust backend (Tauri main process)
├── src/                  # React frontend
└── docs/                 # Original design + implementation plan docs
```

## Tech Stack

- **Desktop**: Tauri 2 (Rust + React + TypeScript + Vite + Tailwind 4)
- **Audio (macOS)**: Swift + ScreenCaptureKit + AVAudioEngine + Core Audio
- **ASR**: Alibaba Paraformer-realtime-v2 (WebSocket streaming)
- **Vector DB**: SQLite + sqlite-vec (1024-dim embeddings)
- **Embedding**: Alibaba text-embedding-v3
- **LLM**: MiniMax-M2.7-highspeed (default) or any OpenAI-compatible API
- **Storage**: Local JSON for keys, SQLite for meetings/transcripts/minutes

## What's Not Here Yet

- Apple Developer code signing (you must right-click → Open on first launch)
- Windows build (scaffolded, not validated)
- Speaker diarization (currently labels as `system` / `mic` only)
- Multi-user / cloud sync
- Mobile

## Contributing

Issues and PRs welcome. Please open an issue first for substantial changes.

## License

[MIT](LICENSE) © 2026 Zion Yang
