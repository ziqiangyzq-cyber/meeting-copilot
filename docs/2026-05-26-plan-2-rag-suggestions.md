# meeting-copilot Plan 2 — RAG + 浮窗智能建议

> Original implementation plan. Project-specific business references neutralized for public release.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** 在 Plan 1 实时转写基础上加 RAG(会议前丢资料 → 切块 + embedding → SQLite)+ 浮窗智能混合建议(每 15-30s 自动 / 快捷键召唤 → 拼 prompt → MiniMax 流式 → 浮窗卡片渲染)。跑完 = "可演示的会议助理 MVP"。

**Architecture:** Tauri Rust 后端加 4 个新模块(`db/`, `rag/`, `llm/`, `suggestion/`),前端拆为 Setup 页 + 浮窗两个 window,通过 Tauri 多窗口 + always-on-top API。

**Tech Stack:** rusqlite + sqlite-vec, pdf-extract, docx-rs, 阿里 text-embedding-v3, MiniMax abab6.5 stream API, Tauri 多窗口 + 全局 shortcut

**Reference:** Design spec `docs/2026-05-26-design.md` §5.4 + §6.2 + §9.2

---

## File Structure(Plan 2 结束时新增)

```
src-tauri/src/
├── db/
│   ├── mod.rs                       (连接池 + migrations 入口)
│   ├── schema.rs                    (SQL DDL + migration runner)
│   └── models.rs                    (Meeting/Material/Chunk/Suggestion structs)
├── rag/
│   ├── mod.rs
│   ├── parser.rs                    (PDF / Word / MD / TXT → text)
│   ├── chunker.rs                   (sentence-aware split,500 字符 + 50 overlap)
│   ├── embedding.rs                 (阿里 text-embedding-v3 client)
│   ├── ingest.rs                    (file → chunks → embeddings → DB)
│   └── retrieve.rs                  (cosine top-K)
├── llm/
│   ├── mod.rs                       (LLMClient trait)
│   └── minimax.rs                   (abab6.5 stream client)
├── suggestion/
│   ├── mod.rs
│   ├── engine.rs                    (timer + 手动召唤 + prompt 拼装)
│   └── prompt.rs                    (智能混合 prompt template)
├── commands.rs                      (扩展:create_meeting, ingest_material, trigger_suggestion)
├── orchestrator.rs                  (扩展:启动时 init SuggestionEngine)
└── lib.rs                           (挂新模块 + 多窗口配置)

src/
├── pages/
│   ├── Setup.tsx                    (会议前表单 + 文件拖拽)
│   └── Floating.tsx                 (浮窗主组件)
├── components/
│   ├── TranscriptView.tsx           (复用 Plan 1)
│   ├── SuggestionCard.tsx           (建议卡片 + 引用标签)
│   ├── MeetingForm.tsx              (会议元数据表单)
│   └── FileDropzone.tsx             (拖拽 + 索引进度)
├── lib/
│   ├── tauri-bridge.ts              (扩展:create_meeting / suggestion stream)
│   └── shortcuts.ts                 (Cmd+Shift+M 等)
├── App.tsx                          (router:Setup ↔ Floating)
└── main.tsx
```

---

## Task 1: DB 层(rusqlite + sqlite-vec + schema)

**Files:**
- Modify: `src-tauri/Cargo.toml`(加 rusqlite + sqlite-vec deps)
- Create: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/db/schema.rs`
- Create: `src-tauri/src/db/models.rs`

- [ ] **Step 1: 加 Cargo deps**

```toml
rusqlite = { version = "0.31", features = ["bundled"] }
sqlite-vec = "0.1"
zerocopy = "0.7"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 2: 写 `db/schema.rs`**

完整 schema(Plan 2 用,Plan 3 加 minutes 表):

```sql
CREATE TABLE IF NOT EXISTS meetings (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  project_ref TEXT,
  purpose TEXT,
  participants TEXT,
  started_at INTEGER NOT NULL,
  ended_at INTEGER,
  audio_path TEXT,
  metadata TEXT
);

CREATE TABLE IF NOT EXISTS materials (
  id TEXT PRIMARY KEY,
  meeting_id TEXT NOT NULL,
  file_name TEXT NOT NULL,
  file_path TEXT NOT NULL,
  file_size INTEGER,
  indexed_at INTEGER,
  chunk_count INTEGER,
  FOREIGN KEY (meeting_id) REFERENCES meetings(id)
);

CREATE TABLE IF NOT EXISTS chunks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  meeting_id TEXT NOT NULL,
  material_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL,
  text TEXT NOT NULL,
  FOREIGN KEY (meeting_id) REFERENCES meetings(id),
  FOREIGN KEY (material_id) REFERENCES materials(id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
  embedding float[1024]
);

CREATE TABLE IF NOT EXISTS transcripts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  meeting_id TEXT NOT NULL,
  speaker TEXT,
  text TEXT NOT NULL,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  is_final INTEGER DEFAULT 0,
  FOREIGN KEY (meeting_id) REFERENCES meetings(id)
);

CREATE TABLE IF NOT EXISTS suggestions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  meeting_id TEXT NOT NULL,
  triggered_at INTEGER NOT NULL,
  trigger_type TEXT,
  style TEXT,
  content TEXT NOT NULL,
  user_action TEXT,
  FOREIGN KEY (meeting_id) REFERENCES meetings(id)
);

CREATE INDEX IF NOT EXISTS idx_chunks_meeting ON chunks(meeting_id);
CREATE INDEX IF NOT EXISTS idx_transcripts_meeting ON transcripts(meeting_id);
```

写 `init_schema(conn) -> Result<()>` 函数,跑上面所有 DDL。`sqlite-vec` extension 在 conn 建立时 `vec0::load(&conn)?` 加载。

- [ ] **Step 3: 写 `db/models.rs`**

按 schema 各表的 Rust struct,加 `From<rusqlite::Row>` 转换。

- [ ] **Step 4: 写 `db/mod.rs`**

```rust
pub mod schema;
pub mod models;

use crate::error::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: PathBuf) -> Result<Self> {
        let conn = Connection::open(&path)?;
        // load sqlite-vec extension
        unsafe { sqlite_vec::sqlite3_vec_init(&conn)?; }
        schema::init(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn conn(&self) -> std::sync::MutexGuard<Connection> {
        self.conn.lock().unwrap()
    }
}
```

`AppError` 加 `Db(#[from] rusqlite::Error)` variant。

- [ ] **Step 5: 单元测试 — init + insert + select**

```rust
#[test]
fn db_init_and_basic_crud() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let db = Db::open(tmp.path().to_path_buf()).unwrap();
    let c = db.conn();
    c.execute("INSERT INTO meetings (id, name, started_at) VALUES (?, ?, ?)",
              params!["m1", "test", 0]).unwrap();
    let count: i64 = c.query_row("SELECT COUNT(*) FROM meetings", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 1);
}
```

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: sqlite + sqlite-vec db layer with full schema"
```

---

## Task 2: 文件解析(PDF / Word / MD / TXT)

**Files:**
- Create: `src-tauri/src/rag/parser.rs`
- Create: `src-tauri/src/rag/mod.rs`
- Modify: `Cargo.toml`(加 pdf-extract, docx-rs)

- [ ] **Step 1: 加 deps**

```toml
pdf-extract = "0.7"
docx-rs = "0.4"
```

- [ ] **Step 2: 写 `parser.rs`**

```rust
use crate::error::{AppError, Result};
use std::path::Path;

pub fn parse(path: &Path) -> Result<String> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    match ext.to_lowercase().as_str() {
        "pdf" => parse_pdf(path),
        "docx" => parse_docx(path),
        "md" | "txt" => parse_text(path),
        _ => Err(AppError::Config(format!("unsupported file type: {ext}"))),
    }
}

fn parse_pdf(path: &Path) -> Result<String> {
    pdf_extract::extract_text(path)
        .map_err(|e| AppError::Config(format!("pdf parse failed: {e}")))
}

fn parse_docx(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let docx = docx_rs::read_docx(&bytes)
        .map_err(|e| AppError::Config(format!("docx parse failed: {e}")))?;
    // 提取所有段落 plain text(忽略格式)
    let mut text = String::new();
    for child in &docx.document.children {
        // ... extract paragraph runs
    }
    Ok(text)
}

fn parse_text(path: &Path) -> Result<String> {
    Ok(std::fs::read_to_string(path)?)
}
```

完整 docx 提取写完(运行时按 docx-rs API 调,详见 docs.rs)。

- [ ] **Step 3: 测试**

每种文件类型一个测试,fixture 在 `tests/fixtures/`(.gitignored,跑测试前生成):
- 小 PDF(用 macOS quartz/cups 生成):`echo "中文测试 hello world" | enscript -o /tmp/t.ps && ps2pdf /tmp/t.ps tests/fixtures/test.pdf`
- 小 docx:用 docx-rs 自己生成一个
- MD/TXT 直接写

- [ ] **Step 4: Commit**

```bash
git commit -m "feat: rag file parser (pdf/docx/md/txt)"
```

---

## Task 3: Chunker(句子边界 + 500 字符 + 50 overlap)

**Files:**
- Create: `src-tauri/src/rag/chunker.rs`

- [ ] **Step 1: 实现 `chunker.rs`**

```rust
pub fn chunk(text: &str, target: usize, overlap: usize) -> Vec<String> {
    // 1. 切句:按 [。！？\n.!?] 切
    // 2. 累积句子直到长度 >= target,产出 chunk
    // 3. 下个 chunk 从 上个 chunk 末尾 - overlap 字符开始
    // 中英文 punctuation 都处理
}
```

- [ ] **Step 2: 5 个测试**

- 短文本(< 500)= 1 chunk
- 长文本(2000 字)= 4 chunks,each ~ 500
- 中英混说切句
- overlap 验证(chunk N 末尾 == chunk N+1 开头 50 字符)
- 边界条件:句子比 target 长

- [ ] **Step 3: Commit**

---

## Task 4: 阿里 text-embedding-v3 client

**Files:**
- Create: `src-tauri/src/rag/embedding.rs`

- [ ] **Step 1: 实现 client**

```rust
pub struct EmbeddingClient {
    api_key: String,
    client: reqwest::Client,
}

impl EmbeddingClient {
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // POST https://dashscope.aliyuncs.com/api/v1/services/embeddings/text-embedding/text-embedding
        // body: { model: "text-embedding-v3", input: { texts: [...] } }
        // 返回 [[f32; 1024], ...]
    }
}
```

加 `reqwest = { version = "0.12", features = ["json"] }` 到 Cargo.toml。

- [ ] **Step 2: 集成测试(用真实 API)**

```rust
#[tokio::test]
#[ignore = "needs ALIYUN_API_KEY"]
async fn embed_chinese_sentences() {
    let key = std::env::var("ALIYUN_API_KEY").unwrap();
    let client = EmbeddingClient::new(key);
    let vecs = client.embed_batch(&[
        "项目 A 的预算估算".into(),
        "产品功能模块".into(),
    ]).await.unwrap();
    assert_eq!(vecs.len(), 2);
    assert_eq!(vecs[0].len(), 1024);
    // 相似度应该 > 0.5(同领域业务相关)
    let sim = cosine(&vecs[0], &vecs[1]);
    assert!(sim > 0.3, "expected similar, got {sim}");
}
```

- [ ] **Step 3: Commit**

---

## Task 5: RAG ingest pipeline + retrieve

**Files:**
- Create: `src-tauri/src/rag/ingest.rs`
- Create: `src-tauri/src/rag/retrieve.rs`

- [ ] **Step 1: `ingest.rs` — 文件 → chunks → embeddings → DB**

```rust
pub async fn ingest_file(
    db: &Db,
    embed: &EmbeddingClient,
    meeting_id: &str,
    file_path: &Path,
) -> Result<MaterialId> {
    let text = parser::parse(file_path)?;
    let chunks = chunker::chunk(&text, 500, 50);
    let embeddings = embed.embed_batch(&chunks).await?;

    // 写 materials 表
    let material_id = Uuid::new_v4().to_string();
    {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO materials (id, meeting_id, file_name, file_path, file_size, indexed_at, chunk_count) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![material_id, meeting_id, file_path.file_name().unwrap().to_string_lossy(), file_path.to_string_lossy(), text.len() as i64, now_ms(), chunks.len() as i64]
        )?;

        // 写 chunks + chunks_vec
        for (i, (text, vec)) in chunks.iter().zip(embeddings.iter()).enumerate() {
            let chunk_id: i64 = conn.query_row(
                "INSERT INTO chunks (meeting_id, material_id, chunk_index, text) VALUES (?, ?, ?, ?) RETURNING id",
                params![meeting_id, material_id, i as i64, text],
                |r| r.get(0)
            )?;
            // chunks_vec 插入(rowid 必须跟 chunks 的 id 匹配,用于检索时 JOIN)
            let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
            conn.execute(
                "INSERT INTO chunks_vec (rowid, embedding) VALUES (?, ?)",
                params![chunk_id, bytes]
            )?;
        }
    }
    Ok(material_id)
}
```

- [ ] **Step 2: `retrieve.rs` — top-K cosine**

```rust
pub async fn retrieve(
    db: &Db,
    embed: &EmbeddingClient,
    meeting_id: &str,
    query: &str,
    k: usize,
) -> Result<Vec<RetrievedChunk>> {
    let q_vec = embed.embed_batch(&[query.to_string()]).await?.remove(0);
    let q_bytes: Vec<u8> = q_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

    let conn = db.conn();
    let mut stmt = conn.prepare("
        SELECT c.id, c.text, c.material_id, m.file_name, v.distance
        FROM chunks_vec v
        JOIN chunks c ON c.id = v.rowid
        JOIN materials m ON m.id = c.material_id
        WHERE c.meeting_id = ?
          AND v.embedding MATCH ?
        ORDER BY v.distance
        LIMIT ?
    ")?;
    // ... 收集结果
}
```

- [ ] **Step 3: 集成测试**

会议 m1 → ingest 一个含"项目报价 211 万"的 .md → 检索 query "对方说报价高" → 验证 top-1 是这条 chunk。

- [ ] **Step 4: Commit**

---

## Task 6: MiniMax LLM client

**Files:**
- Create: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/src/llm/minimax.rs`

- [ ] **Step 1: 定义 `LLMClient` trait**

```rust
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// 流式生成,token-by-token 通过 channel 推
    async fn stream(
        &self,
        system: &str,
        messages: Vec<Message>,
        out: mpsc::Sender<String>,
    ) -> Result<()>;
}

pub struct Message {
    pub role: String,    // "user" | "assistant"
    pub content: String,
}
```

- [ ] **Step 2: `minimax.rs`**

MiniMax abab6.5 stream API:
- POST https://api.minimax.chat/v1/text/chatcompletion_v2
- header `Authorization: Bearer <key>`
- body `{ model: "abab6.5-chat", messages: [{role, content}], stream: true }`
- SSE 响应,每行 `data: {...}\n\n`,parse delta.content
- 加 `MINIMAX_API_KEY` 到 Config

- [ ] **Step 3: 集成测试(需要 MINIMAX_API_KEY)**

```rust
#[tokio::test]
#[ignore]
async fn minimax_stream_chinese_response() {
    // 拿 stream 接收到的 tokens 拼起来,验证 > 5 字
}
```

- [ ] **Step 4: Commit**

---

## Task 7: SuggestionEngine — 触发 + Prompt 拼装

**Files:**
- Create: `src-tauri/src/suggestion/mod.rs`
- Create: `src-tauri/src/suggestion/engine.rs`
- Create: `src-tauri/src/suggestion/prompt.rs`

- [ ] **Step 1: `prompt.rs` — 智能混合模板**

```rust
pub fn build_prompt(
    meta: &MeetingMeta,
    recent_transcript: &str,
    chunks: &[RetrievedChunk],
) -> (String, String) {
    let system = r#"
你是用户的会议 AI 助理。
他在跟客户 / 合作方 / 团队谈判或评审,你帮他做下一步决策辅助。

## 你的输出风格(智能混合)

根据当下对话节奏,从三种风格中选最该给的:
- **战术**(对方刚说一句你不知怎么接)→ 给一句能直接说的话术 + 1 句简短佐证
- **战略**(对方反复绕一个话题 / 在套话)→ 分析对方意图 + 应对方向(具体话他自己说)
- **信息**(对方问数字 / 引规范 / 引项目)→ 直接摆事实数据 + 来源

无论哪种风格,**总长度 < 200 字**,引用资料标签放最后。

## 禁止
- 不要总结"对方说了什么"(他听得到)
- 不要提"建议"二字开头(直接给内容)
- 不要长段落分析
- 中英文混说没问题但别炫
"#;

    let user = format!(r#"
## 会议元数据
{meta_text}

## 最近转写片段(对方=系统音频,我=麦克风)
{transcript}

## 相关资料(检索 top-5)
{chunks_text}

## 任务
看当下,给我一条建议(战术/战略/信息 自选)。
"#,
        meta_text = format_meta(meta),
        transcript = recent_transcript,
        chunks_text = format_chunks(chunks),
    );

    (system.to_string(), user)
}
```

- [ ] **Step 2: `engine.rs` — 维护 transcript window + 触发**

```rust
pub struct SuggestionEngine {
    transcript_buffer: Arc<Mutex<TranscriptBuffer>>,  // 维护最近 N 秒
    db: Arc<Db>,
    embed: Arc<EmbeddingClient>,
    llm: Arc<dyn LLMClient>,
    meeting_id: String,
    meta: MeetingMeta,
}

impl SuggestionEngine {
    /// 入参:刚收到的一段 transcript
    pub async fn push_transcript(&self, evt: TranscriptEvent) {
        self.transcript_buffer.lock().await.push(evt);
    }

    /// 触发一次建议生成(timer or 用户召唤)
    pub async fn generate(&self, trigger: TriggerType, out: mpsc::Sender<String>) -> Result<()> {
        let recent = self.transcript_buffer.lock().await.recent_text(90);  // 最近 90s
        let query = extract_key_sentence(&recent).await?;  // 用 LLM 抽关键句作 RAG query
        let chunks = retrieve::retrieve(&self.db, &self.embed, &self.meeting_id, &query, 5).await?;
        let (system, user) = prompt::build_prompt(&self.meta, &recent, &chunks);
        self.llm.stream(&system, vec![Message::user(user)], out).await
    }

    /// 起 timer,每 N 秒触发一次(后台 task)
    pub fn start_timer(self: Arc<Self>, interval_secs: u64, app: tauri::AppHandle) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                let (tx, mut rx) = mpsc::channel(64);
                let app_clone = app.clone();
                let me = self.clone();
                tokio::spawn(async move {
                    while let Some(token) = rx.recv().await {
                        let _ = app_clone.emit("suggestion_token", token);
                    }
                });
                let _ = self.generate(TriggerType::Auto, tx).await;
                let _ = app.emit("suggestion_complete", ());
            }
        })
    }
}
```

- [ ] **Step 3: 单测 — prompt 拼装 snapshot test**

- [ ] **Step 4: Commit**

---

## Task 8: 扩展 Tauri commands — ingest + suggestion

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`(注册新 commands)
- Modify: `src-tauri/src/orchestrator.rs`(把 SuggestionEngine 装进来)

- [ ] **Step 1: 加 commands**

```rust
#[tauri::command]
pub async fn create_meeting(
    name: String,
    project_ref: Option<String>,
    purpose: Option<String>,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    // INSERT INTO meetings, return id
}

#[tauri::command]
pub async fn ingest_material(
    meeting_id: String,
    file_path: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<String, String> {
    // ingest_file → emit "material_progress" 事件 → return material_id
}

#[tauri::command]
pub async fn trigger_suggestion(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    // 手动召唤,在 SuggestionEngine 上跑一次 generate
}
```

- [ ] **Step 2: Orchestrator 关联 meeting_id + 启 SuggestionEngine timer**

`start_meeting(meeting_id: String)` 改签名,启动时 spawn timer。

- [ ] **Step 3: 测试 commands(用 Tauri mock)**

或跳过,集成层在 Plan 2 末端的 E2E 验证。

- [ ] **Step 4: Commit**

---

## Task 9: Setup 页 + 文件拖拽 UI

**Files:**
- Create: `src/pages/Setup.tsx`
- Create: `src/components/MeetingForm.tsx`
- Create: `src/components/FileDropzone.tsx`

- [ ] **Step 1: `MeetingForm.tsx`**

会议名 / 关联项目 / 会议目的 / 参会人 / "开始会议" 按钮。

- [ ] **Step 2: `FileDropzone.tsx`**

react-dropzone 拖拽 + 文件列表 + 索引进度条(listen `material_progress` event)。

```bash
pnpm add react-dropzone
```

- [ ] **Step 3: `Setup.tsx` 组合**

提交会议表单 → 调 `create_meeting` → 拿到 meeting_id → 拖拽文件 → 调 `ingest_material(meeting_id, path)`(进度从 event 来)→ 全部完成后 → 调 `start_meeting(meeting_id)` → 弹浮窗 / 隐主窗。

- [ ] **Step 4: 视觉验收**(自己跑一遍 `pnpm tauri dev` 看效果)

- [ ] **Step 5: Commit**

---

## Task 10: 浮窗 — Tauri 多窗口 + transparent + always-on-top

**Files:**
- Modify: `src-tauri/tauri.conf.json`(加第二个 window 配置)
- Create: `src/pages/Floating.tsx`
- Modify: `src/App.tsx`(根据 URL 路径分发 Setup 还是 Floating)
- Modify: `src-tauri/src/commands.rs`(加 `open_floating_window` command)

- [ ] **Step 1: tauri.conf.json 加 floating window**

```json
{
  "app": {
    "windows": [
      { "label": "main", "title": "会议助理", "width": 900, "height": 700 },
      {
        "label": "floating",
        "title": "",
        "width": 200,
        "height": 380,
        "transparent": true,
        "decorations": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "visible": false,
        "url": "/floating"
      }
    ]
  }
}
```

- [ ] **Step 2: React Router 设置 `/` → Setup, `/floating` → Floating**

```bash
pnpm add react-router-dom
```

- [ ] **Step 3: `Floating.tsx` 框架**

- 顶栏:状态灯 + 静音 + 结束
- 中上:transcript 滚动
- 中下:建议卡片(下个 task 填)
- 半透明背景(`bg-black/80 backdrop-blur`)

- [ ] **Step 4: `open_floating_window` command + setup 页跳转**

Tauri Rust:
```rust
#[tauri::command]
async fn show_floating(app: tauri::AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window("floating") {
        w.show()?;
    }
    Ok(())
}
```

setup 页点 "开始会议" 后 → 调 `start_meeting` + `show_floating` → 隐主窗 / 显示浮窗。

- [ ] **Step 5: 视觉验收**

- [ ] **Step 6: Commit**

---

## Task 11: 建议卡片 UI + 引用标签 + 召唤快捷键

**Files:**
- Create: `src/components/SuggestionCard.tsx`
- Modify: `src/pages/Floating.tsx`
- Modify: `src/lib/tauri-bridge.ts`(加 suggestion stream listener)
- Modify: `src-tauri/tauri.conf.json`(加 global shortcut plugin)
- Create: `src/lib/shortcuts.ts`

- [ ] **Step 1: SuggestionCard.tsx**

```tsx
interface SuggestionCardProps {
  text: string;        // 流式累积
  isStreaming: boolean;
  refs: ChunkRef[];    // 引用资料
  onRefClick: (id: string) => void;
}

// 渲染:
// - 大字 text(token-by-token 累积)
// - 流式状态显示光标
// - refs 横排 chip
// - 点 chip 弹 dialog 显示原文
```

- [ ] **Step 2: bridge — listen suggestion_token / suggestion_complete**

```typescript
export async function onSuggestion(
  cb: (token: string, done: boolean) => void
): Promise<UnlistenFn> {
  // listen("suggestion_token", ...) + listen("suggestion_complete", ...)
}
```

- [ ] **Step 3: Global shortcut Cmd+Shift+M**

```bash
pnpm add @tauri-apps/plugin-global-shortcut
```

Rust 侧 + JS 侧绑定 Cmd+Shift+M → 调 trigger_suggestion command。

- [ ] **Step 4: 整合 Floating.tsx**

显示最近 1 条建议(完成 + 流式都 OK),按 Cmd+Shift+M 召唤,自动每 15-30s 出新建议时 fade-out 旧的 fade-in 新的。

- [ ] **Step 5: 视觉验收**

- [ ] **Step 6: Commit**

---

## Task 12: 端到端集成测试

- [ ] **Step 1: 准备 fixtures**

写一个测试用 markdown `tests/fixtures/测试报价单.md`(虚拟内容,包含 211 万 / 8 阶段 / 服务范围等),会议中用。

- [ ] **Step 2: 跑一次真实流程**

1. `pnpm tauri dev`
2. setup 页输入会议名 "测试谈判模拟" + 拖入 fixture 文件
3. 等索引完成
4. 点"开始会议" → 浮窗出现
5. 播放音频或自己说话 "对方说我们报价高,你能不能压一压"
6. 看自动建议 / 按 Cmd+Shift+M 召唤
7. 验证建议合理 + 引用了 fixture 资料

- [ ] **Step 3: 5 个验收点**

| # | 项 | 通过标准 |
|---|---|---|
| 1 | 文件索引 | 拖入 → 进度条 → 完成,无错 |
| 2 | 自动建议 | 15-30s 内出第一条建议 |
| 3 | 召唤建议 | Cmd+Shift+M 立刻出建议 |
| 4 | 建议质量 | 建议跟当下转写相关,引用资料命中 |
| 5 | 流式渲染 | 浮窗 token-by-token 显示,不卡 |

- [ ] **Step 4: Commit "test: plan 2 e2e verified"**

---

## Self-Review

(等 plan 全部写完后此 section 由 writing-plans skill 添加)

### Spec 覆盖

| Spec 节 | Plan 2 task |
|---|---|
| §5.4 RAG | Task 1-5 |
| §5.2 SuggestionEngine | Task 7, 8 |
| §6.1 Setup 页 | Task 9 |
| §6.2 浮窗 | Task 10, 11 |
| §9.2 MiniMax | Task 6 |

不在 Plan 2(对应 Plan 3):
- 会议纪要 GPT 生成 → Plan 3
- 历史会议页 → Plan 3
- macOS Keychain → Plan 3
- 录音保留策略 → Plan 3

---

## 已知风险

| 风险 | 备注 |
|---|---|
| sqlite-vec extension 在 bundled rusqlite 里的 init 方式可能要查 | Task 1 step 4 实测时调整 |
| Tauri 多窗口在 macOS 上 always-on-top 的窗口边界行为 | Task 10 视觉验收时调 |
| MiniMax SSE 协议字段(text vs delta)可能跟其他 OpenAI-compat API 不同 | Task 6 实测调 |
| pdf-extract 对中文 PDF / 扫描 PDF 不一定好 | 文档里标"MVP 不支持扫描,提示 OCR" |
| Cmd+Shift+M 跟系统快捷键冲突 | Task 11 验收时换 |

---

**End of Plan 2**
