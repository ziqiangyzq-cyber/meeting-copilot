# AudioHelper

macOS-only Swift CLI that captures system audio (ScreenCaptureKit) + microphone (AVAudioEngine) and outputs 16kHz mono int16 PCM frames on stdout.

Consumed as a subprocess by the meeting-copilot Tauri Rust backend.

## Build

```bash
swift build -c release
```

Binary produced at `.build/release/AudioHelper`.

## Protocol

- **stdin**: line-delimited JSON commands (`{"cmd":"start"}` / `{"cmd":"stop"}` / `{"cmd":"ping"}`)
- **stdout**: binary PCM frames (see Protocol.swift)
- **stderr**: JSON log lines (`{"level":"info","msg":"..."}`)

## Test

```bash
echo '{"cmd":"ping"}' | .build/release/AudioHelper 2>&1 >/dev/null
# Expected stderr: {"level":"info","msg":"pong"}
```

## Permissions

When started with `{"cmd":"start"}`, will request:
- Screen Recording permission (for system audio)
- Microphone permission

Both require user approval via system dialog on first run.
