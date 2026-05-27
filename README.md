# 会议助理 (Meeting Copilot)

一个 macOS 原生会议 AI 助理:实时转写(中英文)→ 智能建议 + 资料引用 → 自动会议纪要。

---

## 给使用者(非开发者)

### 1. 系统要求

- macOS 13+ (Apple Silicon 或 Intel)
- 联网(语音转写 + AI 建议都用云 API)

### 2. 安装

1. 下载 `Meeting Copilot_x.x.x_aarch64.dmg`(从分发渠道拿)
2. 双击打开,把 "Meeting Copilot" 拖到 Applications
3. **首次打开必须右键 → 打开**(因为 app 没有 Apple Developer 签名,直接双击会被 Gatekeeper 拦住):
   - 打开 Applications,**右键** Meeting Copilot
   - 选 "打开"
   - 弹窗里再点"打开"
   - 以后双击就能开了

### 3. 注册 API Key(必须)

两个免费 key 才能工作:

**(a) 阿里 DashScope**(语音转写 + 资料向量化)
- 注册:<https://bailian.console.aliyun.com/cn-beijing?tab=model#/api-key>
- 控制台 → API-KEY 管理 → 创建
- 在阿里**百炼**(Bailian)开通这 2 个模型:
  - `paraformer-realtime-v2`(实时语音转写)
  - `text-embedding-v3`(资料向量化)
- 免费额度够日常用

**(b) MiniMax**
- 注册:<https://platform.minimaxi.com>
- 控制台 → API Key 管理 → 创建
- **必须开通 `MiniMax-M2.7-highspeed`** 这个模型(其他 model 我们没适配)
- 也有免费额度

### 4. 首次使用

1. 启动 app — 自动跳到"首次使用"页面
2. 填进 2 个 Key → 保存到钥匙串
3. macOS 会问 2 个权限:
   - **屏幕录制**(用来抓系统音频,比如 Zoom 对方声音)→ 允许 → 需要重启 app
   - **麦克风** → 允许
4. 开始用

### 5. 使用

- **新建会议**:填会议名 → 可选填关联项目/目的/重点关注 → 可选选会议资料文件夹(自动 RAG 索引)→ 开始
- 主窗左边滚转写,右边滚 AI 建议
- 顶栏可关掉"AI 建议"开关(纯转写模式)
- **重点关注**字段在会议中可以临时改,改完即生效
- 点"结束会议" → 自动生成会议纪要 → 可复制 / 保存 .md
- **历史会议**:回到首页点 📋 → 列表 → 详情可重新生成纪要

### 6. 改 API Key

任何时候,在主页点 ⚙️ → 设置 → 重新输入完整 Key → 保存

### 7. 常见问题

- **"开始会议"没反应** → 检查屏幕录制 + 麦克风权限是不是给了
- **转写没字出来** → 看看是不是阿里 key 错了 / paraformer-realtime-v2 没开通
- **AI 建议总报错** → MiniMax key 错了 / MiniMax-M2.7-highspeed 没开通
- **app 启动闪退** → 在 Terminal 跑 `open -a "Meeting Copilot"` 看错误日志

---

## 给开发者

### 环境

- Node 22+, pnpm 11+, Rust 1.95+, Swift 5.9+, Xcode CLI Tools
- macOS 13+

### Dev 模式

```bash
# 1. 装依赖
pnpm install
cd audio-helper && swift build -c release && cd ..

# 2. API key (Dev fallback,优先于 Keychain)
cp .env.local.example .env.local  # 如果有
# 或者:
export ALIYUN_API_KEY=sk-...
export MINIMAX_API_KEY=sk-cp-...

# 3. 启动 dev
pnpm tauri dev
```

### 打包 .dmg

```bash
pnpm tauri build
# 输出在:src-tauri/target/release/bundle/dmg/Meeting Copilot_x.x.x_aarch64.dmg
```

打包流程自动:
- 编译 Swift AudioHelper(release)
- 复制到 src-tauri/resources/ 让 Tauri bundle 进 .app
- Tauri 生成 .app + .dmg

### Windows 开发 / 打包

**前提**:
- Windows 10 1809+
- Rust toolchain (1.75+)
- MSVC build tools (Visual Studio 2022 Build Tools — Desktop C++ workload)
- Node 22+ + pnpm 11+

**Dev**:

```pwsh
cd meeting-copilot
pnpm install
cd audio-helper-win
cargo build --release
cd ..
$env:ALIYUN_API_KEY = "sk-..."
$env:MINIMAX_API_KEY = "sk-cp-..."
pnpm tauri dev
```

**打包 .msi**:

```pwsh
pnpm tauri build
# 输出在: src-tauri\target\release\bundle\msi\Meeting Copilot_0.1.0_x64_en-US.msi
```

**注意**:Windows 版从未在真实 Windows 机器上测过(代码 from a Mac dev box)。首次跑可能有 1-2 个编译错误,**多半在 `audio-helper-win/src/main.rs` 的 WAVE_FORMAT_EXTENSIBLE 处理 或 `windows` crate API 签名差异**(GUID 检测被简化了)。

### 项目结构

```
meeting-copilot/
├── audio-helper/        # Swift CLI:ScreenCaptureKit + AVAudioEngine 抓音频
├── src-tauri/           # Rust 后端:ASR / RAG / LLM / Orchestrator
├── src/                 # React 前端:Setup / MeetingView / MinutesView / Settings ...
└── docs/                # 设计文档 + Plan 1-4
```

### 文档

- 设计 spec:`docs/2026-05-26-design.md`
- Plan 1(音频 + ASR):`docs/2026-05-26-plan-1-audio-asr.md`
- Plan 2(RAG + 建议):`docs/2026-05-26-plan-2-rag-suggestions.md`

### License

私用 / EFC 内部。
