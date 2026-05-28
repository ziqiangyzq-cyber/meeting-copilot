# AudioHelper (Windows)

Windows version of the meeting-copilot audio helper. Captures system audio (WASAPI loopback on the default render device) + microphone, downmixes to 16 kHz mono int16 LE PCM, and streams frames to stdout using the protocol defined in `audio-helper/Sources/AudioHelper/Protocol.swift`.

## Requirements

- Windows 10 1809+ (WASAPI loopback baseline)
- Rust toolchain (1.75+)
- MSVC build tools (Visual Studio 2022 Build Tools — Desktop C++ workload)

## Build

```pwsh
cd audio-helper-win
cargo build --release
```

Output: `target/release/AudioHelper.exe` (~1-2 MB)

## Protocol

Identical to the macOS Swift version:

- **stdin**: line-delimited JSON commands (`{"cmd":"start"}`, `{"cmd":"stop"}`, `{"cmd":"ping"}`)
- **stdout**: binary frames
  - 4 bytes magic `0xAB12CD34` LE
  - 4 bytes source tag LE (`0` = system audio, `1` = microphone)
  - 4 bytes payload size LE (uint32)
  - N bytes PCM int16 LE 16 kHz mono
- **stderr**: JSON log lines (`{"level":"info","msg":"..."}`)

## Known Limitations

- **Linear resampler** — adequate for ASR but not audiophile-grade.
- **Approximate float-format detection** — skips GUID inspection for `WAVE_FORMAT_EXTENSIBLE`. Most Win10/11 shared-mode mix formats are 32-bit float, so we assume float when `wBitsPerSample == 32`. If a Windows tester reports garbled audio, the proper fix is to cast to `WAVEFORMATEXTENSIBLE` and inspect the `SubFormat` GUID.
- **5 ms polling** — slightly higher CPU than event-driven WASAPI; fine for MVP.
- **Untested on real hardware** — this code was written from a Mac dev box and has never been compiled or run on Windows. First Windows build may surface 1-2 compile errors, most likely in the `windows` crate API surface area for `IAudioClient::Initialize` or `IAudioCaptureClient::GetBuffer` (signatures evolve between `windows` crate versions).

## License

MIT — see [LICENSE](../LICENSE) at the repo root.
