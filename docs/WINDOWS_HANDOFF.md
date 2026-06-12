# Meeting Copilot — Windows 移植 Handoff

**给在 Windows 机器上接手这个项目的 AI 助手 / 开发者**

## 项目背景

这是一个 macOS + Windows 跨平台桌面 app:实时会议语音转写(中英)+ AI 智能建议 + 会议纪要。

- **桌面框架**:Tauri 2.x(Rust 后端 + React 前端)
- **macOS 版**:✅ 完成 + 已打包 .dmg,Swift AudioHelper 抓系统音频
- **Windows 版**:🚧 代码层完成,**从未在真实 Windows 编译/测试**

你的工作:把 Windows 版跑通。

## 项目结构(精简版)

```
meeting-copilot/
├── audio-helper/           # macOS 专属 Swift CLI(用 ScreenCaptureKit + AVAudioEngine)
├── audio-helper-win/       # ⭐ Windows 专属 Rust + WASAPI CLI(你的工作重点)
├── src-tauri/              # Tauri Rust 后端 — 业务逻辑(ASR / RAG / LLM / Orchestrator)
├── src/                    # React 前端(Setup / MeetingView / MinutesView / Settings ...)
└── README.md               # 用户/开发者文档
```

## 你需要的 Windows 环境

- **Windows 10 1809+** 或 Windows 11
- **Rust toolchain 1.75+**(<https://rustup.rs>)
- **MSVC build tools**:Visual Studio 2022 Build Tools — 勾选 "Desktop development with C++" workload
- **Node.js 22+** + **pnpm 11+**(<https://pnpm.io/installation>)
- Git(可选,如果要 commit)

## 快速开始

```pwsh
# 1. 解压源码
tar -xzf meeting-copilot-source.tar.gz
cd meeting-copilot

# 2. 装前端依赖
pnpm install

# 3. 编译 Windows AudioHelper(关键步骤)
cd audio-helper-win
cargo build --release
# ⚠️ 这一步**大概率会失败** — 见下面"已知问题"
```

## ⚠️ 已知问题(必读)

### 1. `windows` crate API 签名漂移

`audio-helper-win/src/main.rs` 用的是 `windows = "0.58"` crate(微软官方 Win32 绑定),代码是凭印象写的,**没有在真实 Windows 上编译过**。

最可能的错误:
- `CoInitializeEx` 返回类型(HRESULT vs Result<()>)
- `IAudioClient::Initialize` 参数顺序 / Option wrapping
- `IAudioCaptureClient::GetBuffer` 的 out-param 方式
- `device.Activate()` 的泛型类型推断
- `enumerator.GetDefaultAudioEndpoint` 第 1 个参数(`EDataFlow` enum vs int)

**修法**:看 `cargo build` 错误信息 + 查 <https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/Audio/index.html>,逐个改 signature。每个错都是 1-3 行 fix。

### 2. WAVE_FORMAT_EXTENSIBLE 处理简化了

当前代码假设 32-bit 采样就是 float32。Windows 10/11 大部分混音器是 float32,**但某些 pro-audio 硬件可能在 WAVE_FORMAT_EXTENSIBLE 里返回 int16**。

如果你跑通后发现声音乱码(转写出来全是噪音字符),fix 在 `audio-helper-win/src/main.rs`,把 `is_float` 判断改为检查 `WAVEFORMATEXTENSIBLE.SubFormat` GUID(`KSDATAFORMAT_SUBTYPE_IEEE_FLOAT` vs `KSDATAFORMAT_SUBTYPE_PCM`)。

### 3. Resampler 是线性的

够用,不必动。

## 测试流程

编译通过后:

```pwsh
# 1. 测 AudioHelper 单独跑(stdin/stdout 协议测试)
echo {"cmd":"ping"} | .\audio-helper-win\target\release\AudioHelper.exe
# 应该 stderr 看到: {"level":"info","msg":"pong"}

# 2. 测真实抓音频(需要 mic 权限)
echo {"cmd":"start"} | .\audio-helper-win\target\release\AudioHelper.exe > test.pcm 2> test.log
# 让它跑 10 秒,期间播放音乐 + 说话
# 检查 test.log 没有 error,test.pcm 文件非空(应该几百 KB)

# 3. 检查 PCM 帧结构(用 Python)
python -c "
import struct
with open('test.pcm', 'rb') as f:
    data = f.read()
i = 0
counts = {}
while i + 12 <= len(data):
    magic, src, size = struct.unpack('<III', data[i:i+12])
    if magic != 0xAB12CD34:
        print(f'BAD MAGIC at offset {i}'); break
    counts[src] = counts.get(src, 0) + 1
    i += 12 + size
print('system frames:', counts.get(0, 0), '| mic frames:', counts.get(1, 0))
"
# 应该 system 和 mic 都有几十到几百帧
```

### 4. 集成测试(整个 app)

```pwsh
cd meeting-copilot
$env:ALIYUN_API_KEY = "sk-..."     # 用户已经有这个 key
$env:MINIMAX_API_KEY = "sk-cp-..." # 用户已经有这个 key
pnpm tauri dev
```

主窗出来后:
1. 首次跳"设置"页填入 2 个 key,点 ⚙️ 旁边的 "测试" 按钮验证 key 有效
2. 保存 → 进 Setup 页
3. 创建会议 → 开始 → 看转写有没有滚出来
4. 测中文 + 英文 + 中英混
5. 结束会议 → 看纪要生成

## 打包 .msi

跑通后:
```pwsh
pnpm tauri build
# 输出: src-tauri\target\release\bundle\msi\Meeting Copilot_0.1.0_x64_en-US.msi
```

## 文件清单(给你参考改哪些)

| 你大概率要改 | 改什么 |
|---|---|
| `audio-helper-win/src/main.rs` | `windows` crate signature 修正(2-4 处) |
| `audio-helper-win/Cargo.toml` | 如果 windows crate 版本升级了,改 = "0.58" → 当前稳定版 |

| 你大概率**不**用动(除非有问题) | 备注 |
|---|---|
| `src-tauri/src/orchestrator.rs::locate_helper_binary` | 已经按 OS 选 `AudioHelper` 或 `AudioHelper.exe`,应该自动工作 |
| `src-tauri/scripts/bundle-helper.mjs` | 跨平台 dispatcher,Win 上自动调 PowerShell 版 |
| `src-tauri/scripts/bundle-audio-helper.ps1` | PowerShell 构建脚本,你 cargo build 通了就不用动 |
| `src-tauri/tauri.conf.json` | resources/* glob 已配,Win 自动包含 .exe |

## 全局架构(2 分钟版)

```
[ScreenCaptureKit (macOS) or WASAPI (Win)] → AudioHelper binary 抓 PCM
   ↓ stdout binary frames (magic+src+size+payload)
[Tauri Rust] HelperProc 解帧
   ↓ PCM
[阿里 Paraformer WebSocket] 实时 ASR
   ↓ transcript events
[SuggestionEngine] 每 20s 拿 90s 转写 → RAG 检索 → MiniMax LLM → token stream
   ↓
[Tauri events: transcript / suggestion_token / suggestion_complete / minutes_token / ...]
   ↓
[React 前端] 主窗 2 栏视图(转写左 / 建议右)
```

API:
- **阿里 DashScope**(<https://bailian.console.aliyun.com/cn-beijing?tab=model#/api-key>):paraformer-realtime-v2(语音) + text-embedding-v3(向量)
- **MiniMax**(<https://platform.minimaxi.com>):MiniMax-M2.7-highspeed(建议 + 纪要 + 翻译)

## 不要做什么

- 不要改 macOS Swift 助手(`audio-helper/`)— Mac 版已工作
- 不要改 React 前端逻辑 — 已 stable
- 不要改 Plan 1-5 的功能边界 — 只让 Windows 跑通
- 不要打包未测试通过的 .msi — 先 dev 跑通再说

## 完成 / 给用户的反馈格式

跑通后,告诉用户:
1. 哪些 signature 错误你修了(列 file:line + before/after)
2. 测试结果:转写是否准确,有没有掉帧,延迟多大
3. 打包是否出了 .msi,大小多大
4. 如果有未解决的问题,具体描述

如果 1 天搞不定,把卡住的 cargo build 错误贴出来,问用户。

---

**良好运气 🚀**
