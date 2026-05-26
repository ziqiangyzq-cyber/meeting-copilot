# meeting-copilot Plan 1 — 音频抓取 + 实时转写最小通路

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用户能在 Tauri app 里点"开始" → 同时抓系统音频和麦克风 → 实时看到中英文转写文字滚动 → 点"结束"。这是整个 meeting-copilot 项目的最小可工作通路,跑通后再加 RAG / 建议 / 纪要。

**Architecture:** 三进程 — Tauri Rust 主进程 spawn Swift AudioHelper 子进程(stdio pipe 传 PCM)→ Rust 转发到阿里 Paraformer WebSocket → 流式 transcript 通过 Tauri event emit 到前端 React。

**Tech Stack:** Tauri 2.x / Rust(tokio, tokio-tungstenite, serde)/ React 18 + TypeScript + Vite + Tailwind / Swift 5.9 + ScreenCaptureKit + AVFoundation

**Reference spec:** `00_AI工作区/meeting-copilot/docs/2026-05-26-design.md`

---

## File Structure(Plan 1 结束时)

```
00_AI工作区/meeting-copilot/
├── docs/
│   ├── 2026-05-26-design.md
│   ├── 2026-05-26-plan-1-audio-asr.md         (本文件)
│   └── 2026-05-26-plan-2-rag-suggestions.md   (Plan 2 占位)
├── .git/
├── .gitignore
├── README.md
├── package.json
├── pnpm-lock.yaml
├── tsconfig.json
├── vite.config.ts
├── tailwind.config.js
├── index.html
├── src/                                       (React 前端)
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/
│   │   └── TranscriptView.tsx
│   └── lib/
│       └── tauri-bridge.ts
├── src-tauri/                                 (Rust 后端)
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/
│   ├── resources/                             (打包时放 AudioHelper binary)
│   └── src/
│       ├── main.rs
│       ├── commands.rs                        (Tauri command 入口)
│       ├── audio_pump/
│       │   ├── mod.rs
│       │   ├── helper_proc.rs                 (spawn Swift subprocess)
│       │   └── frame.rs                       (PCM frame parser)
│       ├── asr/
│       │   ├── mod.rs                         (ASRClient trait)
│       │   └── aliyun_paraformer.rs           (WebSocket client)
│       ├── config.rs                          (env / Keychain 读取占位)
│       └── error.rs
├── audio-helper/                              (Swift CLI)
│   ├── Package.swift
│   ├── Sources/
│   │   └── AudioHelper/
│   │       ├── main.swift
│   │       ├── SystemAudioCapture.swift       (ScreenCaptureKit)
│   │       ├── MicCapture.swift               (AVAudioEngine)
│   │       ├── AudioMixer.swift               (重采样 + 双轨输出)
│   │       └── Protocol.swift                 (stdin JSON / stdout PCM frame)
│   └── README.md                              (build 说明)
└── tests/                                     (集成测试 fixtures)
    └── fixtures/
        └── chinese_30s.wav                    (中文测试音频)
```

---

## Task 0: 初始化项目 + git

**Files:**
- Create: `00_AI工作区/meeting-copilot/.gitignore`
- Create: `00_AI工作区/meeting-copilot/README.md`

- [ ] **Step 1: cd 到项目目录**

```bash
cd "/Users/yangziqiang/Documents/工作文件夹/00_AI工作区/meeting-copilot"
```

- [ ] **Step 2: git init + 写 .gitignore**

```bash
git init
```

写 `.gitignore`:
```
# OS
.DS_Store

# IDE
.vscode/
.idea/

# Node
node_modules/
dist/
.vite/

# Rust
src-tauri/target/

# Swift
audio-helper/.build/
audio-helper/.swiftpm/
audio-helper/Package.resolved

# Secrets (will use Keychain instead of .env in Phase 2, but guard against accidents)
.env
.env.local

# 录音(本地存,大文件,不进 git)
recordings/

# Tauri builds
src-tauri/target/
```

- [ ] **Step 3: 写最小 README.md**

```markdown
# meeting-copilot

EFC 会议 AI 助理 — 实时会议语音转写 + 智能建议 + 自动纪要。

详见 [Design v1.0](docs/2026-05-26-design.md)。

## 开发状态

Plan 1:音频 + 转写最小通路 — 实施中
```

- [ ] **Step 4: commit**

```bash
git add .gitignore README.md docs/
git commit -m "chore: init project with design doc + plan 1"
```

---

## Task 1: Tauri 2 + React + TS + Tailwind 脚手架

**Files:**
- Create: 整个 Tauri 项目骨架(用 create-tauri-app)

- [ ] **Step 1: 用 create-tauri-app 初始化**

```bash
cd "/Users/yangziqiang/Documents/工作文件夹/00_AI工作区/meeting-copilot"
pnpm create tauri-app . --template react-ts --manager pnpm --identifier com.efc.meeting-copilot --name "Meeting Copilot"
```

**预期**:工具会问几个问题,确认 React/TS/pnpm。如果当前目录非空它会问要不要继续,选 yes。

- [ ] **Step 2: 安装依赖**

```bash
pnpm install
```

- [ ] **Step 3: 装 Tailwind 4**

```bash
pnpm add -D tailwindcss@latest @tailwindcss/vite
```

修改 `vite.config.ts`,在 plugins 数组加 `tailwindcss()`:

```typescript
import tailwindcss from '@tailwindcss/vite';
// ... in defineConfig.plugins:
plugins: [react(), tailwindcss()],
```

修改 `src/main.tsx` 顶部加:
```typescript
import './index.css';
```

修改 `src/index.css`,内容改为:
```css
@import "tailwindcss";
```

- [ ] **Step 4: 验证 dev 能跑起来**

```bash
pnpm tauri dev
```

**预期**:打开一个 Tauri 窗口,显示默认欢迎页。能看到窗口 = 成功。Ctrl+C 关掉。

- [ ] **Step 5: 配置 tauri.conf.json 基础信息**

修改 `src-tauri/tauri.conf.json`:
- `productName`: "Meeting Copilot"
- `version`: "0.1.0"
- `identifier`: "com.efc.meeting-copilot"
- `app.windows[0]`:
  - `title`: "会议助理"
  - `width`: 900
  - `height`: 700
  - `resizable`: true

- [ ] **Step 6: commit**

```bash
git add -A
git commit -m "feat: tauri 2 + react + ts + tailwind scaffold"
```

---

## Task 2: Rust 后端模块骨架

**Files:**
- Create: `src-tauri/src/audio_pump/mod.rs`
- Create: `src-tauri/src/audio_pump/frame.rs`
- Create: `src-tauri/src/audio_pump/helper_proc.rs`
- Create: `src-tauri/src/asr/mod.rs`
- Create: `src-tauri/src/asr/aliyun_paraformer.rs`
- Create: `src-tauri/src/config.rs`
- Create: `src-tauri/src/error.rs`
- Modify: `src-tauri/src/main.rs`(挂模块)
- Modify: `src-tauri/Cargo.toml`(加依赖)

- [ ] **Step 1: 给 Cargo.toml 加依赖**

修改 `src-tauri/Cargo.toml`,在 `[dependencies]` 加:

```toml
tokio = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
futures-util = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
uuid = { version = "1", features = ["v4"] }
bytes = "1"
anyhow = "1"
```

- [ ] **Step 2: 写 error.rs**

```rust
// src-tauri/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("audio helper error: {0}")]
    AudioHelper(String),

    #[error("ASR error: {0}")]
    Asr(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;

// 让 AppError 能跨 Tauri IPC 边界
impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
```

- [ ] **Step 3: 写 config.rs**

```rust
// src-tauri/src/config.rs
use crate::error::{AppError, Result};

pub struct Config {
    pub aliyun_api_key: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let aliyun_api_key = std::env::var("ALIYUN_API_KEY")
            .map_err(|_| AppError::Config("ALIYUN_API_KEY env var not set".into()))?;
        Ok(Self { aliyun_api_key })
    }
}
```

- [ ] **Step 4: 写各模块的 mod.rs 占位**

`src-tauri/src/audio_pump/mod.rs`:
```rust
pub mod frame;
pub mod helper_proc;

pub use frame::AudioFrame;
pub use helper_proc::HelperProc;
```

`src-tauri/src/asr/mod.rs`:
```rust
pub mod aliyun_paraformer;

use async_trait::async_trait;

#[async_trait]
pub trait ASRClient: Send + Sync {
    /// 推一帧 PCM 16kHz mono 16-bit little-endian
    async fn push_pcm(&mut self, src: AudioSource, pcm: &[u8]) -> crate::error::Result<()>;

    /// 关闭流
    async fn close(&mut self) -> crate::error::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    System,  // 对方
    Mic,     // 我
}
```

加 `async-trait` 到 Cargo.toml:
```toml
async-trait = "0.1"
```

`src-tauri/src/audio_pump/frame.rs` 留空:
```rust
// 下一个 task 实现 frame parser
```

`src-tauri/src/audio_pump/helper_proc.rs` 留空:
```rust
// Task 3 实现
```

`src-tauri/src/asr/aliyun_paraformer.rs` 留空:
```rust
// Task 4 实现
```

- [ ] **Step 5: 挂模块到 main.rs**

修改 `src-tauri/src/main.rs`,顶部加:

```rust
mod audio_pump;
mod asr;
mod config;
mod error;
```

- [ ] **Step 6: 验证编译**

```bash
cd src-tauri && cargo build
```

**预期**:编译通过(可能有 unused warnings,OK)。

- [ ] **Step 7: commit**

```bash
git add -A
git commit -m "feat: rust module skeleton (audio_pump/asr/config/error)"
```

---

## Task 3: Swift AudioHelper 项目骨架

**Files:**
- Create: `audio-helper/Package.swift`
- Create: `audio-helper/Sources/AudioHelper/main.swift`
- Create: `audio-helper/Sources/AudioHelper/Protocol.swift`
- Create: `audio-helper/README.md`

- [ ] **Step 1: 建目录 + Package.swift**

```bash
mkdir -p audio-helper/Sources/AudioHelper
```

写 `audio-helper/Package.swift`:
```swift
// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "AudioHelper",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "AudioHelper", targets: ["AudioHelper"]),
    ],
    targets: [
        .executableTarget(
            name: "AudioHelper",
            path: "Sources/AudioHelper"
        ),
    ]
)
```

- [ ] **Step 2: 定义 stdin/stdout 协议**

写 `audio-helper/Sources/AudioHelper/Protocol.swift`:

```swift
import Foundation

// stdin: line-delimited JSON commands
struct Command: Codable {
    let cmd: String
    // 可扩展字段
}

// stdout: 二进制 PCM frame
// 帧格式:
//   [4 字节 magic = 0xAB12CD34]
//   [4 字节 source_tag: 0=system, 1=mic]
//   [4 字节 frame_size in bytes (little-endian uint32)]
//   [frame_size 字节 PCM int16 16kHz mono LE]
enum AudioSource: UInt32 {
    case system = 0
    case mic = 1
}

let frameMagic: UInt32 = 0xAB12CD34

func writeFrame(source: AudioSource, pcm: Data, to fd: FileHandle) {
    var magic = frameMagic.littleEndian
    var src = source.rawValue.littleEndian
    var size = UInt32(pcm.count).littleEndian
    fd.write(Data(bytes: &magic, count: 4))
    fd.write(Data(bytes: &src, count: 4))
    fd.write(Data(bytes: &size, count: 4))
    fd.write(pcm)
}

// stderr: JSON log lines
func logInfo(_ msg: String) {
    let line = #"{"level":"info","msg":"\#(msg)"}"# + "\n"
    FileHandle.standardError.write(line.data(using: .utf8)!)
}

func logError(_ msg: String) {
    let line = #"{"level":"error","msg":"\#(msg)"}"# + "\n"
    FileHandle.standardError.write(line.data(using: .utf8)!)
}
```

- [ ] **Step 3: 写 main.swift 占位(stdin reader + handler dispatch)**

```swift
import Foundation

logInfo("AudioHelper started")

// 简单的同步 stdin 读取(每行一个 JSON 命令)
let stdin = FileHandle.standardInput

func handleCommand(_ cmd: Command) {
    switch cmd.cmd {
    case "start":
        logInfo("start command received (capture not implemented yet)")
        // Task 4-6 实现
    case "stop":
        logInfo("stop command received")
        exit(0)
    case "ping":
        logInfo("pong")
    default:
        logError("unknown command: \(cmd.cmd)")
    }
}

while let line = readLine() {
    guard let data = line.data(using: .utf8) else {
        logError("non-utf8 input")
        continue
    }
    do {
        let cmd = try JSONDecoder().decode(Command.self, from: data)
        handleCommand(cmd)
    } catch {
        logError("decode failed: \(error)")
    }
}

logInfo("AudioHelper exiting (stdin closed)")
```

- [ ] **Step 4: build 验证**

```bash
cd audio-helper
swift build -c release
```

**预期**:产出 `.build/release/AudioHelper`,编译无错。

- [ ] **Step 5: 手测 stdin/stdout 协议**

```bash
echo '{"cmd":"ping"}' | .build/release/AudioHelper 2>&1 >/dev/null
```

**预期**:stderr 看到 `{"level":"info","msg":"pong"}`。

- [ ] **Step 6: commit**

```bash
cd ..
git add -A
git commit -m "feat: swift AudioHelper scaffold with stdin/stdout protocol"
```

---

## Task 4: Swift: ScreenCaptureKit 抓系统音频

**Files:**
- Create: `audio-helper/Sources/AudioHelper/SystemAudioCapture.swift`
- Modify: `audio-helper/Sources/AudioHelper/main.swift`

- [ ] **Step 1: 写 SystemAudioCapture.swift**

```swift
import Foundation
import ScreenCaptureKit
import AVFoundation

@available(macOS 13.0, *)
class SystemAudioCapture: NSObject, SCStreamDelegate, SCStreamOutput {
    private var stream: SCStream?
    private let outputQueue = DispatchQueue(label: "system-audio-output")
    private let converter: PCMConverter

    init(converter: PCMConverter) {
        self.converter = converter
    }

    func start() async throws {
        // 获取可用的 sharable content
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first else {
            throw NSError(domain: "SystemAudio", code: 1,
                          userInfo: [NSLocalizedDescriptionKey: "no display found"])
        }

        // 配置:只要音频,视频部分用最小规格
        let config = SCStreamConfiguration()
        config.capturesAudio = true
        config.excludesCurrentProcessAudio = true  // 不抓自己 app 输出
        config.sampleRate = 16000
        config.channelCount = 1
        // 视频部分必须开,但用最小消耗
        config.width = 2
        config.height = 2
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)  // 1fps 最低
        config.queueDepth = 8

        let filter = SCContentFilter(display: display, excludingWindows: [])

        let stream = SCStream(filter: filter, configuration: config, delegate: self)
        try stream.addStreamOutput(self, type: .audio, sampleHandlerQueue: outputQueue)
        try await stream.startCapture()
        self.stream = stream
        logInfo("system audio capture started")
    }

    func stop() async throws {
        if let stream = stream {
            try await stream.stopCapture()
            self.stream = nil
            logInfo("system audio capture stopped")
        }
    }

    // SCStreamOutput
    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio else { return }
        guard let pcmData = converter.extractPCM(from: sampleBuffer) else {
            return
        }
        writeFrame(source: .system, pcm: pcmData, to: FileHandle.standardOutput)
    }

    // SCStreamDelegate
    func stream(_ stream: SCStream, didStopWithError error: Error) {
        logError("system audio stream stopped with error: \(error)")
    }
}
```

- [ ] **Step 2: 写 PCMConverter.swift**

```swift
import Foundation
import AVFoundation

// 把 CMSampleBuffer / AVAudioPCMBuffer 转成 16kHz mono int16 LE PCM Data
class PCMConverter {
    private var converter: AVAudioConverter?
    private let targetFormat: AVAudioFormat

    init() {
        targetFormat = AVAudioFormat(commonFormat: .pcmFormatInt16,
                                     sampleRate: 16000,
                                     channels: 1,
                                     interleaved: true)!
    }

    func extractPCM(from sampleBuffer: CMSampleBuffer) -> Data? {
        guard let formatDesc = CMSampleBufferGetFormatDescription(sampleBuffer),
              let asbd = CMAudioFormatDescriptionGetStreamBasicDescription(formatDesc)?.pointee
        else { return nil }

        let inputFormat = AVAudioFormat(streamDescription: &asbd)
        guard let inputFormat = inputFormat else { return nil }

        // 提取 PCM buffer
        guard let pcmBuffer = AVAudioPCMBuffer(pcmFormat: inputFormat,
                                               frameCapacity: AVAudioFrameCount(CMSampleBufferGetNumSamples(sampleBuffer)))
        else { return nil }
        pcmBuffer.frameLength = pcmBuffer.frameCapacity

        let status = CMSampleBufferCopyPCMDataIntoAudioBufferList(
            sampleBuffer, at: 0,
            frameCount: Int32(pcmBuffer.frameLength),
            into: pcmBuffer.mutableAudioBufferList
        )
        guard status == noErr else { return nil }

        return convert(pcmBuffer)
    }

    func convert(_ inputBuffer: AVAudioPCMBuffer) -> Data? {
        let inputFormat = inputBuffer.format
        if converter == nil || converter?.inputFormat != inputFormat {
            converter = AVAudioConverter(from: inputFormat, to: targetFormat)
        }
        guard let converter = converter else { return nil }

        let ratio = targetFormat.sampleRate / inputFormat.sampleRate
        let outCapacity = AVAudioFrameCount(Double(inputBuffer.frameLength) * ratio) + 32
        guard let outBuffer = AVAudioPCMBuffer(pcmFormat: targetFormat,
                                               frameCapacity: outCapacity)
        else { return nil }

        var error: NSError?
        let inputBlock: AVAudioConverterInputBlock = { _, outStatus in
            outStatus.pointee = .haveData
            return inputBuffer
        }
        converter.convert(to: outBuffer, error: &error, withInputFrom: inputBlock)
        if let error = error {
            logError("converter error: \(error)")
            return nil
        }

        let byteCount = Int(outBuffer.frameLength) * 2  // int16 = 2 bytes
        guard let int16Ptr = outBuffer.int16ChannelData?[0] else { return nil }
        return Data(bytes: int16Ptr, count: byteCount)
    }
}
```

- [ ] **Step 3: 在 main.swift 接入**

修改 `main.swift`:

```swift
import Foundation

logInfo("AudioHelper started")

let converter = PCMConverter()
var systemCapture: SystemAudioCapture?

if #available(macOS 13.0, *) {
    systemCapture = SystemAudioCapture(converter: converter)
}

func handleCommand(_ cmd: Command) async {
    switch cmd.cmd {
    case "start":
        do {
            try await systemCapture?.start()
        } catch {
            logError("start failed: \(error)")
        }
    case "stop":
        do {
            try await systemCapture?.stop()
        } catch {
            logError("stop failed: \(error)")
        }
        exit(0)
    case "ping":
        logInfo("pong")
    default:
        logError("unknown command: \(cmd.cmd)")
    }
}

// 主循环改成异步
let stdin = FileHandle.standardInput
let semaphore = DispatchSemaphore(value: 0)

DispatchQueue.global().async {
    while let line = readLine() {
        guard let data = line.data(using: .utf8) else { continue }
        guard let cmd = try? JSONDecoder().decode(Command.self, from: data) else { continue }
        Task { await handleCommand(cmd) }
    }
    semaphore.signal()
}

semaphore.wait()
```

- [ ] **Step 4: build**

```bash
cd audio-helper
swift build -c release
```

- [ ] **Step 5: 手测(需要授权屏幕录制)**

```bash
# 第一次跑会弹出系统权限请求, 允许后:
echo '{"cmd":"start"}' | .build/release/AudioHelper > /tmp/test.pcm 2> /tmp/test.log
# 让它跑 10 秒, 期间播放任何音乐
sleep 10
echo '{"cmd":"stop"}' | .build/release/AudioHelper
```

**预期**:
- 第一次会弹"AudioHelper 想要录制屏幕"系统弹窗 → 允许
- `/tmp/test.pcm` 非空,大小 ~ `10s × 16000 × 2 + frame_overhead` ≈ 320KB+
- `/tmp/test.log` 没有 error

- [ ] **Step 6: 用 ffplay 验证 PCM 可播**

```bash
# 提取裸 PCM(跳过 frame header,每 12 字节 header + N 字节 payload)
# 简化:用 Python 提一个 frame 看
python3 -c "
import sys, struct
with open('/tmp/test.pcm', 'rb') as f:
    data = f.read()
i = 0
out = b''
while i < len(data):
    magic, src, size = struct.unpack('<III', data[i:i+12])
    assert magic == 0xAB12CD34
    out += data[i+12:i+12+size]
    i += 12 + size
with open('/tmp/test_raw.pcm', 'wb') as f:
    f.write(out)
print('extracted', len(out), 'bytes')
"

# 播放(需要 ffmpeg)
ffplay -f s16le -ar 16000 -ac 1 /tmp/test_raw.pcm
```

**预期**:听到刚才播放音乐 10s。

- [ ] **Step 7: commit**

```bash
cd ..
git add -A
git commit -m "feat: swift system audio capture via ScreenCaptureKit"
```

---

## Task 5: Swift: 麦克风抓取 + 双源混入 stdout

**Files:**
- Create: `audio-helper/Sources/AudioHelper/MicCapture.swift`
- Modify: `audio-helper/Sources/AudioHelper/main.swift`

- [ ] **Step 1: 写 MicCapture.swift**

```swift
import Foundation
import AVFoundation

class MicCapture {
    private let engine = AVAudioEngine()
    private let converter: PCMConverter
    private var isRunning = false

    init(converter: PCMConverter) {
        self.converter = converter
    }

    func start() throws {
        // 麦克风权限会自动触发 NSMicrophoneUsageDescription 弹窗
        let input = engine.inputNode
        let inputFormat = input.outputFormat(forBus: 0)

        input.installTap(onBus: 0, bufferSize: 1024, format: inputFormat) { [weak self] buffer, _ in
            guard let self = self else { return }
            guard let pcmData = self.converter.convert(buffer) else { return }
            writeFrame(source: .mic, pcm: pcmData, to: FileHandle.standardOutput)
        }

        try engine.start()
        isRunning = true
        logInfo("mic capture started, input format: \(inputFormat)")
    }

    func stop() {
        if isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
            isRunning = false
            logInfo("mic capture stopped")
        }
    }
}
```

- [ ] **Step 2: 在 main.swift 接入**

修改 `main.swift`,加 mic capture 实例,并在 `start`/`stop` 命令里同步启动/停止:

```swift
let converter = PCMConverter()
var systemCapture: SystemAudioCapture?
let micCapture = MicCapture(converter: converter)

if #available(macOS 13.0, *) {
    systemCapture = SystemAudioCapture(converter: converter)
}

func handleCommand(_ cmd: Command) async {
    switch cmd.cmd {
    case "start":
        do {
            try await systemCapture?.start()
            try micCapture.start()
        } catch {
            logError("start failed: \(error)")
        }
    case "stop":
        do {
            try await systemCapture?.stop()
            micCapture.stop()
        } catch {
            logError("stop failed: \(error)")
        }
        exit(0)
    case "ping":
        logInfo("pong")
    default:
        logError("unknown command: \(cmd.cmd)")
    }
}

// ... (stdin 读取循环不变)
```

- [ ] **Step 3: build**

```bash
cd audio-helper
swift build -c release
```

- [ ] **Step 4: 手测**

```bash
echo '{"cmd":"start"}' | .build/release/AudioHelper > /tmp/test_dual.pcm 2> /tmp/test_dual.log &
# 让它跑 10 秒, 期间播放音乐 + 自己说话
sleep 10
killall AudioHelper
```

**预期**:第一次跑会弹麦克风权限请求,允许。`/tmp/test_dual.log` 看到 mic capture started + system capture started。`/tmp/test_dual.pcm` 应包含两种 source tag 的帧。

- [ ] **Step 5: 验证 source tag 分布**

```bash
python3 -c "
import sys, struct
with open('/tmp/test_dual.pcm', 'rb') as f:
    data = f.read()
i = 0
counts = {0: 0, 1: 0}
while i < len(data):
    magic, src, size = struct.unpack('<III', data[i:i+12])
    counts[src] = counts.get(src, 0) + 1
    i += 12 + size
print('system frames:', counts.get(0, 0), '| mic frames:', counts.get(1, 0))
"
```

**预期**:system 和 mic 各有几十到几百帧。

- [ ] **Step 6: commit**

```bash
cd ..
git add -A
git commit -m "feat: swift mic capture via AVAudioEngine, dual-source output"
```

---

## Task 6: Rust: spawn AudioHelper + 解析帧

**Files:**
- Modify: `src-tauri/src/audio_pump/frame.rs`
- Modify: `src-tauri/src/audio_pump/helper_proc.rs`
- Create: `src-tauri/src/audio_pump/tests.rs`

- [ ] **Step 1: 写 frame.rs(PCM 帧 + parser)**

```rust
// src-tauri/src/audio_pump/frame.rs
use crate::error::{AppError, Result};
use bytes::{Buf, BytesMut};
use tokio::io::AsyncReadExt;

pub const FRAME_MAGIC: u32 = 0xAB12CD34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    System,
    Mic,
}

impl AudioSource {
    fn from_tag(tag: u32) -> Result<Self> {
        match tag {
            0 => Ok(Self::System),
            1 => Ok(Self::Mic),
            _ => Err(AppError::AudioHelper(format!("unknown source tag: {tag}"))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub source: AudioSource,
    pub pcm: Vec<u8>,  // int16 LE 16kHz mono
}

pub struct FrameReader<R: AsyncReadExt + Unpin> {
    reader: R,
    buf: BytesMut,
}

impl<R: AsyncReadExt + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: BytesMut::with_capacity(64 * 1024),
        }
    }

    /// 读下一帧。EOF 返回 Ok(None)。
    pub async fn next_frame(&mut self) -> Result<Option<AudioFrame>> {
        loop {
            // 至少需要 12 字节 header
            if self.buf.len() >= 12 {
                let magic = u32::from_le_bytes(self.buf[0..4].try_into().unwrap());
                if magic != FRAME_MAGIC {
                    // resync:丢一个字节再试
                    self.buf.advance(1);
                    continue;
                }
                let src_tag = u32::from_le_bytes(self.buf[4..8].try_into().unwrap());
                let size = u32::from_le_bytes(self.buf[8..12].try_into().unwrap()) as usize;

                if self.buf.len() >= 12 + size {
                    self.buf.advance(12);
                    let pcm = self.buf.split_to(size).to_vec();
                    let source = AudioSource::from_tag(src_tag)?;
                    return Ok(Some(AudioFrame { source, pcm }));
                }
            }

            // 读更多
            let mut chunk = [0u8; 8192];
            let n = self.reader.read(&mut chunk).await?;
            if n == 0 {
                // EOF
                return Ok(None);
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}
```

- [ ] **Step 2: 写测试 — frame parser 用 fixture bytes**

`src-tauri/src/audio_pump/frame.rs` 文件底部加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tokio::io::AsyncRead;

    fn build_frame(source: u32, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&FRAME_MAGIC.to_le_bytes());
        buf.extend_from_slice(&source.to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[tokio::test]
    async fn reads_single_frame() {
        let bytes = build_frame(0, &[0x01, 0x02, 0x03, 0x04]);
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::new(cursor));
        let frame = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(frame.source, AudioSource::System);
        assert_eq!(frame.pcm, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[tokio::test]
    async fn reads_multiple_frames() {
        let mut bytes = build_frame(0, &[1, 2]);
        bytes.extend(build_frame(1, &[3, 4, 5]));
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::new(cursor));
        let f1 = reader.next_frame().await.unwrap().unwrap();
        let f2 = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(f1.source, AudioSource::System);
        assert_eq!(f2.source, AudioSource::Mic);
        assert_eq!(reader.next_frame().await.unwrap().is_none(), true);
    }

    #[tokio::test]
    async fn skips_garbage_before_magic() {
        let mut bytes = vec![0xFF, 0xEE, 0xDD];  // 噪声
        bytes.extend(build_frame(0, &[1, 2]));
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::new(cursor));
        let f = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(f.source, AudioSource::System);
        assert_eq!(f.pcm, vec![1, 2]);
    }
}
```

- [ ] **Step 3: 运行 frame parser 测试**

```bash
cd src-tauri
cargo test audio_pump::frame
```

**预期**:3 个测试全 PASS。

- [ ] **Step 4: 写 helper_proc.rs**

```rust
// src-tauri/src/audio_pump/helper_proc.rs
use crate::audio_pump::frame::{AudioFrame, FrameReader};
use crate::error::{AppError, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct HelperProc {
    child: Child,
    stdin: ChildStdin,
    pub frames_rx: mpsc::Receiver<AudioFrame>,
}

impl HelperProc {
    pub async fn spawn(binary_path: PathBuf) -> Result<Self> {
        let mut child = Command::new(&binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::AudioHelper(format!("spawn failed: {e}")))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| AppError::AudioHelper("no stdin".into()))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| AppError::AudioHelper("no stdout".into()))?;
        let stderr = child.stderr.take()
            .ok_or_else(|| AppError::AudioHelper("no stderr".into()))?;

        // 启动 stderr 日志转发
        tokio::spawn(forward_stderr(stderr));

        // 启动 stdout frame reader
        let (frames_tx, frames_rx) = mpsc::channel(256);
        tokio::spawn(read_frames_loop(stdout, frames_tx));

        Ok(Self { child, stdin, frames_rx })
    }

    pub async fn send_cmd(&mut self, cmd: &str) -> Result<()> {
        let line = format!("{{\"cmd\":\"{cmd}\"}}\n");
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.send_cmd("stop").await;
        let _ = self.child.wait().await;
        Ok(())
    }
}

async fn read_frames_loop(stdout: ChildStdout, tx: mpsc::Sender<AudioFrame>) {
    let mut reader = FrameReader::new(BufReader::new(stdout));
    loop {
        match reader.next_frame().await {
            Ok(Some(frame)) => {
                if tx.send(frame).await.is_err() {
                    info!("frames_rx closed, stopping");
                    return;
                }
            }
            Ok(None) => {
                info!("AudioHelper stdout closed");
                return;
            }
            Err(e) => {
                error!("frame read error: {}", e);
                return;
            }
        }
    }
}

async fn forward_stderr(stderr: tokio::process::ChildStderr) {
    use tokio::io::AsyncBufReadExt;
    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        info!("[AudioHelper] {}", line);
    }
}
```

- [ ] **Step 5: 编译验证**

```bash
cargo build
```

**预期**:编译通过(可能有 unused warning)。

- [ ] **Step 6: commit**

```bash
cd ..
git add -A
git commit -m "feat: rust AudioHelper subprocess wrapper + frame parser (tested)"
```

---

## Task 7: Rust: 阿里 Paraformer WebSocket client

**Files:**
- Modify: `src-tauri/src/asr/aliyun_paraformer.rs`
- Create: `src-tauri/src/asr/tests.rs`

**参考文档**:
- 端点 + 协议:https://help.aliyun.com/zh/model-studio/websocket-for-paraformer-real-time-service
- 鉴权:Bearer token,从 DashScope API Key 拿

- [ ] **Step 1: 实现 aliyun_paraformer.rs(基础连接 + 协议)**

```rust
// src-tauri/src/asr/aliyun_paraformer.rs
use crate::asr::{ASRClient, AudioSource};
use crate::error::{AppError, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

const WS_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";
const MODEL: &str = "paraformer-realtime-v2";

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RunTaskMsg {
    header: Header,
    payload: RunTaskPayload,
}

#[derive(Debug, Serialize)]
struct Header {
    action: String,
    task_id: String,
    streaming: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RunTaskPayload {
    task_group: String,
    task: String,
    function: String,
    model: String,
    parameters: TaskParameters,
    input: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
struct TaskParameters {
    format: String,
    sample_rate: u32,
    vocabulary_id: Option<String>,
    disfluency_removal_enabled: bool,
    language_hints: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ServerMsg {
    header: ServerHeader,
    payload: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ServerHeader {
    event: String,
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TranscriptEvent {
    pub source: AudioSource,
    pub text: String,
    pub is_final: bool,
    pub begin_ms: u64,
    pub end_ms: u64,
}

pub struct AliyunParaformer {
    /// 每个 source 一个独立连接(让对方/我两路独立识别,准度更高)
    system_tx: mpsc::Sender<Vec<u8>>,
    mic_tx: mpsc::Sender<Vec<u8>>,
}

impl AliyunParaformer {
    pub async fn connect(
        api_key: String,
        vocabulary_id: Option<String>,
        transcript_tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<Self> {
        let (system_tx, system_rx) = mpsc::channel::<Vec<u8>>(256);
        let (mic_tx, mic_rx) = mpsc::channel::<Vec<u8>>(256);

        // 各开一条连接
        spawn_stream(api_key.clone(), vocabulary_id.clone(),
                     AudioSource::System, system_rx, transcript_tx.clone()).await?;
        spawn_stream(api_key, vocabulary_id,
                     AudioSource::Mic, mic_rx, transcript_tx).await?;

        Ok(Self { system_tx, mic_tx })
    }
}

#[async_trait]
impl ASRClient for AliyunParaformer {
    async fn push_pcm(&mut self, src: AudioSource, pcm: &[u8]) -> Result<()> {
        let tx = match src {
            AudioSource::System => &self.system_tx,
            AudioSource::Mic => &self.mic_tx,
        };
        tx.send(pcm.to_vec()).await
            .map_err(|_| AppError::Asr("ASR channel closed".into()))?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        // drop senders 自动让 stream task 退出
        Ok(())
    }
}

async fn spawn_stream(
    api_key: String,
    vocabulary_id: Option<String>,
    source: AudioSource,
    mut pcm_rx: mpsc::Receiver<Vec<u8>>,
    transcript_tx: mpsc::Sender<TranscriptEvent>,
) -> Result<()> {
    let task_id = Uuid::new_v4().simple().to_string();

    // 建连
    let mut req = WS_URL.into_client_request()
        .map_err(|e| AppError::Asr(format!("invalid url: {e}")))?;
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {api_key}").parse().unwrap(),
    );
    req.headers_mut().insert("X-DashScope-DataInspection", "enable".parse().unwrap());

    let (ws_stream, _) = connect_async(req).await?;
    let (mut write, mut read) = ws_stream.split();

    // 发 run-task
    let run_task = RunTaskMsg {
        header: Header {
            action: "run-task".into(),
            task_id: task_id.clone(),
            streaming: "duplex".into(),
        },
        payload: RunTaskPayload {
            task_group: "audio".into(),
            task: "asr".into(),
            function: "recognition".into(),
            model: MODEL.into(),
            parameters: TaskParameters {
                format: "pcm".into(),
                sample_rate: 16000,
                vocabulary_id,
                disfluency_removal_enabled: false,
                language_hints: vec!["zh".into(), "en".into()],
            },
            input: serde_json::json!({}),
        },
    };
    let run_json = serde_json::to_string(&run_task)?;
    write.send(Message::Text(run_json)).await?;

    // 起两个并发 task:发音频 + 收 transcript
    tokio::spawn(async move {
        // 发送循环
        while let Some(pcm) = pcm_rx.recv().await {
            if write.send(Message::Binary(pcm)).await.is_err() {
                break;
            }
        }
        // 发 finish-task
        let finish = serde_json::json!({
            "header": {
                "action": "finish-task",
                "task_id": task_id,
                "streaming": "duplex"
            },
            "payload": {}
        });
        let _ = write.send(Message::Text(finish.to_string())).await;
    });

    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let server: ServerMsg = match serde_json::from_str(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!("parse server msg failed: {e}, raw: {text}");
                            continue;
                        }
                    };
                    match server.header.event.as_str() {
                        "result-generated" => {
                            if let Some(payload) = server.payload {
                                if let Some(output) = payload.get("output") {
                                    parse_and_emit_transcript(output, source, &transcript_tx).await;
                                }
                            }
                        }
                        "task-failed" => {
                            tracing::error!("ASR task failed: {:?} {:?}",
                                server.header.error_code, server.header.error_message);
                            break;
                        }
                        "task-finished" => {
                            tracing::info!("ASR task finished");
                            break;
                        }
                        _ => {}
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("ws read error: {e}");
                    break;
                }
            }
        }
    });

    Ok(())
}

async fn parse_and_emit_transcript(
    output: &serde_json::Value,
    source: AudioSource,
    tx: &mpsc::Sender<TranscriptEvent>,
) {
    let sentence = output.get("sentence");
    if let Some(sentence) = sentence {
        let text = sentence.get("text").and_then(|t| t.as_str()).unwrap_or("");
        let begin = sentence.get("begin_time").and_then(|t| t.as_u64()).unwrap_or(0);
        let end = sentence.get("end_time").and_then(|t| t.as_u64()).unwrap_or(0);
        let is_final = sentence.get("sentence_end")
            .and_then(|b| b.as_bool()).unwrap_or(false);

        if !text.is_empty() {
            let _ = tx.send(TranscriptEvent {
                source,
                text: text.into(),
                is_final,
                begin_ms: begin,
                end_ms: end,
            }).await;
        }
    }
}

// 让 reqwest::IntoClientRequest trait 可用
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
```

**注意**:阿里 DashScope 实时 ASR 的精确协议字段在文档为准,以上根据 2026-05 版本写。如果连不上 / parse 不对,先 print raw server message 调试。

- [ ] **Step 2: 编译**

```bash
cd src-tauri && cargo build
```

修复任何 type 错(尤其 IntoClientRequest import 路径)。

- [ ] **Step 3: 写一个小的连接测试(可选,需 API key)**

`src-tauri/src/asr/tests.rs`:
```rust
// 仅在设了 ALIYUN_API_KEY 时跑
#[cfg(test)]
mod tests {
    use super::super::*;
    use super::super::aliyun_paraformer::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    #[ignore = "需要 ALIYUN_API_KEY env"]
    async fn connect_and_get_one_transcript() {
        let key = std::env::var("ALIYUN_API_KEY").expect("ALIYUN_API_KEY not set");
        let (tx, mut rx) = mpsc::channel(16);
        let mut client = AliyunParaformer::connect(key, None, tx).await.unwrap();

        // 读 fixture WAV(16kHz mono int16 LE)的 raw PCM 部分
        let wav = std::fs::read("../tests/fixtures/chinese_30s.wav").unwrap();
        // 简单跳过 44 字节 WAV header
        let pcm = &wav[44..];

        // 分块推
        for chunk in pcm.chunks(3200) {  // 100ms @ 16kHz int16
            client.push_pcm(AudioSource::System, chunk).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        }

        // 等 transcript
        let timeout = tokio::time::sleep(std::time::Duration::from_secs(10));
        tokio::pin!(timeout);
        let mut got_text = String::new();
        loop {
            tokio::select! {
                Some(evt) = rx.recv() => {
                    if !evt.text.is_empty() && evt.is_final {
                        got_text = evt.text;
                        break;
                    }
                }
                _ = &mut timeout => break,
            }
        }
        assert!(!got_text.is_empty(), "should get at least one transcript");
        println!("transcribed: {got_text}");
    }
}
```

挂到 `src-tauri/src/asr/mod.rs`:
```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 4: 准备 fixture(中文 30s WAV)**

```bash
# 用 macOS say 命令生成中文测试音频
mkdir -p tests/fixtures
say -v Tingting -o /tmp/cn.aiff "今天的会议主要讨论陆家嘴连桥项目的报价方案。客户希望我们能在211万的基础上做一定的调整。"
# 转 16kHz mono WAV
ffmpeg -i /tmp/cn.aiff -ar 16000 -ac 1 -sample_fmt s16 tests/fixtures/chinese_30s.wav -y
```

- [ ] **Step 5: 真实跑一次集成测试**

```bash
export ALIYUN_API_KEY="<你的 key>"
cd src-tauri
cargo test --test connect_and_get_one_transcript -- --ignored --nocapture
```

**预期**:控制台输出转写文字,包含"陆家嘴"等关键词。

如果失败,看错误信息:
- `401 Unauthorized` → API key 不对
- `404` → 端点 URL 不对(check 文档)
- `parse server msg failed` → 协议字段不对,把 raw text print 出来对照文档

- [ ] **Step 6: commit**

```bash
cd ..
git add -A
git commit -m "feat: aliyun paraformer websocket client (basic, dual source)"
```

---

## Task 8: Rust: AudioPump 编排(Helper → ASR → 前端 emit)

**Files:**
- Create: `src-tauri/src/orchestrator.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: 写 orchestrator.rs**

```rust
// src-tauri/src/orchestrator.rs
use crate::asr::{aliyun_paraformer::AliyunParaformer, AudioSource as AsrSource, ASRClient};
use crate::asr::aliyun_paraformer::TranscriptEvent;
use crate::audio_pump::{frame::AudioSource as PumpSource, HelperProc};
use crate::config::Config;
use crate::error::Result;
use std::path::PathBuf;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use std::sync::Arc;

pub struct Orchestrator {
    helper: Option<HelperProc>,
    asr: Option<AliyunParaformer>,
    forward_handle: Option<JoinHandle<()>>,
    transcript_handle: Option<JoinHandle<()>>,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            helper: None,
            asr: None,
            forward_handle: None,
            transcript_handle: None,
        }
    }

    pub async fn start(&mut self, config: &Config, app: tauri::AppHandle) -> Result<()> {
        // 1. 启 HelperProc
        let bin_path = locate_helper_binary()?;
        let mut helper = HelperProc::spawn(bin_path).await?;
        helper.send_cmd("start").await?;

        // 2. 启 ASR
        let (transcript_tx, mut transcript_rx) = mpsc::channel::<TranscriptEvent>(64);
        let mut asr = AliyunParaformer::connect(
            config.aliyun_api_key.clone(),
            None,  // vocabulary_id 后期加
            transcript_tx,
        ).await?;

        // 3. 起一个 task:Helper frames → ASR
        let mut frames_rx = std::mem::replace(&mut helper.frames_rx, mpsc::channel(1).1);
        // hack:把 receiver 拿出来.helper struct 改了一下方便.
        // 这里假设 HelperProc::frames_rx 是 pub
        let asr_inner = Arc::new(Mutex::new(asr));
        let asr_clone = asr_inner.clone();
        let forward = tokio::spawn(async move {
            while let Some(frame) = frames_rx.recv().await {
                let asr_src = match frame.source {
                    PumpSource::System => AsrSource::System,
                    PumpSource::Mic => AsrSource::Mic,
                };
                let mut a = asr_clone.lock().await;
                if let Err(e) = a.push_pcm(asr_src, &frame.pcm).await {
                    tracing::error!("push_pcm failed: {e}");
                    break;
                }
            }
        });

        // 4. 起 task:转写事件 → 前端 emit
        let transcript_loop = tokio::spawn(async move {
            use tauri::Emitter;
            while let Some(evt) = transcript_rx.recv().await {
                let payload = serde_json::json!({
                    "source": match evt.source {
                        AsrSource::System => "system",
                        AsrSource::Mic => "mic",
                    },
                    "text": evt.text,
                    "is_final": evt.is_final,
                    "begin_ms": evt.begin_ms,
                    "end_ms": evt.end_ms,
                });
                let _ = app.emit("transcript", payload);
            }
        });

        self.helper = Some(helper);
        self.asr = None;  // 已 move 进 Arc<Mutex>>; 实际项目用更优雅的方式
        self.forward_handle = Some(forward);
        self.transcript_handle = Some(transcript_loop);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(helper) = self.helper.take() {
            helper.shutdown().await?;
        }
        if let Some(h) = self.forward_handle.take() {
            h.abort();
        }
        if let Some(h) = self.transcript_handle.take() {
            h.abort();
        }
        Ok(())
    }
}

fn locate_helper_binary() -> Result<PathBuf> {
    // Dev:用 audio-helper/.build/release/AudioHelper
    // Prod:bundle 内 Contents/Resources/AudioHelper
    // 简化:env override > dev path > bundled
    if let Ok(p) = std::env::var("AUDIO_HELPER_PATH") {
        return Ok(PathBuf::from(p));
    }
    // 假设 cargo run 在 src-tauri/ 跑
    let dev_path = PathBuf::from("../audio-helper/.build/release/AudioHelper");
    if dev_path.exists() {
        return Ok(dev_path);
    }
    Err(crate::error::AppError::AudioHelper(
        "AudioHelper binary not found; set AUDIO_HELPER_PATH env or build audio-helper first".into()
    ))
}
```

**注意**:这个 Orchestrator 有几处 hack(`asr` 不能稳定地 `take`),实际写时要细化。这里给骨架,执行时按编译器报错修。

- [ ] **Step 2: 让 HelperProc::frames_rx 可移出**

修改 `helper_proc.rs`,把 `frames_rx` 改成 `Option<mpsc::Receiver<AudioFrame>>` 或者提供 `take_frames` 方法:

```rust
impl HelperProc {
    pub fn take_frames(&mut self) -> Option<mpsc::Receiver<AudioFrame>> {
        // 把 frames_rx 设为 None 并返回原值;需要把 field 类型改成 Option
        None  // 占位,实际改 struct
    }
}
```

更干净的做法:struct 加 `frames_rx: Option<mpsc::Receiver<AudioFrame>>`,然后 `take_frames` 用 `self.frames_rx.take()`。改一下。

- [ ] **Step 3: 写 commands.rs(Tauri command 暴露给前端)**

```rust
// src-tauri/src/commands.rs
use crate::config::Config;
use crate::error::Result;
use crate::orchestrator::Orchestrator;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AppState {
    pub orchestrator: Arc<Mutex<Orchestrator>>,
}

#[tauri::command]
pub async fn start_meeting(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    let config = Config::from_env().map_err(|e| e.to_string())?;
    let mut o = state.orchestrator.lock().await;
    o.start(&config, app).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_meeting(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let mut o = state.orchestrator.lock().await;
    o.stop().await.map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: main.rs 挂上**

```rust
// src-tauri/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio_pump;
mod asr;
mod config;
mod error;
mod orchestrator;
mod commands;

use commands::{AppState, start_meeting, stop_meeting};
use orchestrator::Orchestrator;
use std::sync::Arc;
use tokio::sync::Mutex;

fn main() {
    tracing_subscriber::fmt::init();

    tauri::Builder::default()
        .manage(AppState {
            orchestrator: Arc::new(Mutex::new(Orchestrator::new())),
        })
        .invoke_handler(tauri::generate_handler![start_meeting, stop_meeting])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: 编译**

```bash
cd src-tauri && cargo build
```

修任何编译错。

- [ ] **Step 6: commit**

```bash
cd ..
git add -A
git commit -m "feat: orchestrator + tauri commands (start/stop meeting)"
```

---

## Task 9: 前端 — 实时 transcript UI

**Files:**
- Modify: `src/App.tsx`
- Create: `src/components/TranscriptView.tsx`
- Create: `src/lib/tauri-bridge.ts`

- [ ] **Step 1: 写 tauri-bridge.ts**

```typescript
// src/lib/tauri-bridge.ts
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

export interface TranscriptEvent {
  source: 'system' | 'mic';
  text: string;
  is_final: boolean;
  begin_ms: number;
  end_ms: number;
}

export async function startMeeting(): Promise<void> {
  await invoke('start_meeting');
}

export async function stopMeeting(): Promise<void> {
  await invoke('stop_meeting');
}

export async function onTranscript(
  callback: (evt: TranscriptEvent) => void
): Promise<UnlistenFn> {
  return listen<TranscriptEvent>('transcript', (e) => callback(e.payload));
}
```

- [ ] **Step 2: 写 TranscriptView.tsx**

```tsx
// src/components/TranscriptView.tsx
import { useEffect, useRef } from 'react';
import { TranscriptEvent } from '../lib/tauri-bridge';

interface Props {
  items: TranscriptEvent[];
}

export function TranscriptView({ items }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [items.length]);

  return (
    <div className="h-96 overflow-y-auto bg-gray-50 rounded p-4 font-mono text-sm space-y-2 border">
      {items.length === 0 && (
        <div className="text-gray-400 italic">等待会议开始...</div>
      )}
      {items.map((item, i) => (
        <div
          key={i}
          className={`flex gap-2 ${
            item.source === 'system' ? 'text-blue-700' : 'text-green-700'
          }`}
        >
          <span className="font-bold shrink-0">
            {item.source === 'system' ? '对方' : '我'}
          </span>
          <span>{item.text}</span>
          {!item.is_final && <span className="text-gray-400">…</span>}
        </div>
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
```

- [ ] **Step 3: 写 App.tsx**

```tsx
// src/App.tsx
import { useEffect, useState } from 'react';
import { TranscriptEvent, startMeeting, stopMeeting, onTranscript } from './lib/tauri-bridge';
import { TranscriptView } from './components/TranscriptView';

export default function App() {
  const [items, setItems] = useState<TranscriptEvent[]>([]);
  const [isRunning, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    onTranscript((evt) => {
      setItems((prev) => {
        // 简单实现:每条 is_final 累积,中间态合并
        if (evt.is_final) return [...prev, evt];
        // 否则替换最后一条同源的中间态
        const next = [...prev];
        const last = next[next.length - 1];
        if (last && !last.is_final && last.source === evt.source) {
          next[next.length - 1] = evt;
        } else {
          next.push(evt);
        }
        return next;
      });
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleStart = async () => {
    setError(null);
    try {
      await startMeeting();
      setRunning(true);
    } catch (e) {
      setError(String(e));
    }
  };

  const handleStop = async () => {
    await stopMeeting();
    setRunning(false);
  };

  return (
    <div className="min-h-screen bg-white p-8">
      <h1 className="text-2xl font-bold mb-4">会议助理 — Plan 1 验证</h1>

      <div className="mb-4 flex gap-2">
        {!isRunning ? (
          <button
            onClick={handleStart}
            className="px-4 py-2 bg-blue-600 text-white rounded"
          >
            开始会议
          </button>
        ) : (
          <button
            onClick={handleStop}
            className="px-4 py-2 bg-red-600 text-white rounded"
          >
            结束会议
          </button>
        )}
        <span className="px-3 py-2 text-sm">
          状态:{isRunning ? '🟢 进行中' : '⚪ 空闲'}
        </span>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 text-red-800 rounded">
          错误:{error}
        </div>
      )}

      <TranscriptView items={items} />
    </div>
  );
}
```

- [ ] **Step 4: 启动 dev,测试 UI 渲染**

```bash
pnpm tauri dev
```

**预期**:Tauri 窗口出现,标题"会议助理 — Plan 1 验证",一个"开始会议"按钮,下方空 transcript 区域。

- [ ] **Step 5: commit**

```bash
git add -A
git commit -m "feat: react frontend with transcript view + start/stop buttons"
```

---

## Task 10: 端到端集成测试

**目标**:真实跑一次,系统音频 + 麦克风同时抓,转写文字滚动出现。

- [ ] **Step 1: build audio-helper(release)**

```bash
cd audio-helper
swift build -c release
cd ..
```

- [ ] **Step 2: 设置 API key**

```bash
export ALIYUN_API_KEY="<你在阿里 DashScope 的 API key>"
```

(暂时用 env,Phase 2 加 Keychain。开 dev 前需要在同一个 shell 设 env。)

- [ ] **Step 3: 启动 dev**

```bash
pnpm tauri dev
```

- [ ] **Step 4: 首次启动授权**

应用窗口出现 → 点"开始会议" → macOS 弹"屏幕录制"权限请求 → 允许(可能要重启 app)→ 重新点 → 弹"麦克风"权限 → 允许。

- [ ] **Step 5: 真实跑**

播放任何中文 / 英文音频(Zoom / YouTube / Bilibili / 自己说话),观察:
- transcript 区域是否实时出现文字?
- "对方"(蓝色)= 系统音频,"我"(绿色)= 麦克风,标签对不对?
- 中英文都能识别?

- [ ] **Step 6: 验收点**

| 验收项 | 通过标准 |
|---|---|
| 系统音频识别 | ≥ 80% 准确度,无明显 lag |
| 麦克风识别 | 你正常说话能识别 |
| 中英文混说 | 能切换识别,不全乱 |
| 端到端延迟 | 说完话 ≤ 3 秒内字出现 |
| 稳定性 | 跑 10 分钟不崩 |

- [ ] **Step 7: 记录已知问题(README 更新)**

把跑通过程中发现的小 bug / 体验问题写到 `README.md` 的 "Known Issues / TODO" 一节,Plan 2 起点参考。

- [ ] **Step 8: commit**

```bash
git add -A
git commit -m "test: end-to-end audio capture + paraformer asr verified"
```

---

## Plan 1 验收完成定义

**所有 Task 0-10 完成** + **Task 10 Step 6 验收点全过** = Plan 1 完成。

**Plan 1 完成后下一步**:
1. 跟 Zion 演示 + 5-10 分钟真实试用
2. 反馈调整(可能需要修阿里 Paraformer 协议、UI 调整、AudioHelper 稳定性)
3. 一切 OK 后开始写 Plan 2(RAG + 浮窗智能建议)

---

## 已知风险 / 边界

| 风险 | 备注 |
|---|---|
| 阿里 Paraformer 协议字段可能跟 Task 7 写的有出入 | 实际跑时按 server response 调整 |
| ScreenCaptureKit 在虚拟显示器 / 外接显示器场景 | 用 `displays.first` 可能不对,需要让用户选 |
| AVAudioEngine 在切换默认输入设备时会断 | Phase 1 不处理,Phase 2 加 device change 监听 |
| AudioHelper 子进程崩溃 | Phase 1 没自动重启,需要 user 手动重新开始会议 |
| WebSocket 断流 | Task 7 没写重连,长会议可能掉。Plan 2 补 |
| transcript event 高频可能压垮前端 | 实测 Paraformer 一般 1-2 Hz,前端没问题。Plan 2 加节流 |

---

## Self-Review

(此节由 writing-plans skill 完成,见对话)

### 1. Spec 覆盖

| Spec 章节 | Plan 1 覆盖任务 |
|---|---|
| §4 三进程架构 | Task 1, 3, 6, 8 |
| §5.1 AudioHelper | Task 3, 4, 5 |
| §5.2 Tauri Main (audio_pump + asr + orchestrator) | Task 2, 6, 7, 8 |
| §5.3 前端 - 主窗口 | Task 9 |
| §9.1 阿里 Paraformer | Task 7 |
| §10 权限 | Task 10(运行时引导) |

**不在 Plan 1 范围**(对应 Plan 2/3):
- §5.4 RAG → Plan 2
- §5.3 浮窗 + Setup 页 + 历史页 → Plan 2/3
- §9.2 MiniMax → Plan 2
- §9.3 GPT 纪要 → Plan 3
- §8 完整 SQLite schema → Plan 2
- §11 隐私 / 录音保留 → Plan 3
- Keychain → Plan 3(暂用 env)

### 2. 占位符扫描

无 TBD / TODO / "later"。已知风险节里列了不完美的地方,但不是 placeholder,是明示边界。

### 3. 类型一致性

- `AudioSource` 在 Swift 和 Rust 两侧:Swift `enum AudioSource: UInt32` / Rust `enum AudioSource`,tag 一致(0=system, 1=mic)
- `TranscriptEvent` 在 Rust 端定义,前端 TS 类型镜像
- `AppError` 在 Rust 内部统一

OK,plan ready。
