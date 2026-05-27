use crate::db::models::{Meeting, SuggestionRow, TranscriptRow};
use crate::llm::Message;
use crate::minutes::generator::MinutesGenerator;
use crate::orchestrator::Orchestrator;
use crate::rag::ingest;
use rusqlite::params;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Serialize)]
pub struct MeetingSummary {
    pub id: String,
    pub name: String,
    pub project_ref: Option<String>,
    pub purpose: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub transcript_count: i64,
    pub suggestion_count: i64,
    pub has_minutes: bool,
}

#[derive(Serialize)]
pub struct MeetingDetail {
    pub meeting: Meeting,
    pub transcripts: Vec<TranscriptRow>,
    pub suggestions: Vec<SuggestionRow>,
    pub latest_minutes_md: Option<String>,
    pub latest_minutes_version: Option<i64>,
}

pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
}

#[tauri::command]
pub async fn create_meeting(
    name: String,
    project_ref: Option<String>,
    purpose: Option<String>,
    participants: Option<String>,
    focus_points: Option<String>,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    let meeting_id = Uuid::new_v4().simple().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let db = state.orchestrator.db();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO meetings (id, name, project_ref, purpose, participants, started_at, focus_points) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![meeting_id, name, project_ref, purpose, participants, now, focus_points],
    )
    .map_err(|e| e.to_string())?;

    Ok(meeting_id)
}

#[tauri::command]
pub async fn update_focus_points(
    meeting_id: String,
    focus_points: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    let db = state.orchestrator.db();
    let conn = db.conn();
    // Empty string means "no focus" — store as NULL for cleanliness
    let value: Option<String> = if focus_points.trim().is_empty() {
        None
    } else {
        Some(focus_points)
    };
    conn.execute(
        "UPDATE meetings SET focus_points = ? WHERE id = ?",
        params![value, meeting_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn ingest_material(
    meeting_id: String,
    file_path: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<String, String> {
    if !crate::config::keys_configured() {
        return Err("API key 未配置,请先在 ⚙️ 设置里填入".into());
    }
    let path = PathBuf::from(&file_path);

    let _ = app.emit(
        "material_progress",
        serde_json::json!({
            "file_path": file_path,
            "status": "started"
        }),
    );

    let db = state.orchestrator.db();
    let embed = state.orchestrator.embed();
    let result = ingest::ingest_file(&db, &embed, &meeting_id, &path).await;

    match result {
        Ok(material_id) => {
            let _ = app.emit(
                "material_progress",
                serde_json::json!({
                    "file_path": file_path,
                    "status": "completed",
                    "material_id": material_id.clone()
                }),
            );
            Ok(material_id)
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = app.emit(
                "material_progress",
                serde_json::json!({
                    "file_path": file_path,
                    "status": "failed",
                    "error": msg.clone()
                }),
            );
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn start_meeting(
    meeting_id: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    if !crate::config::keys_configured() {
        return Err("API key 未配置,请先在 ⚙️ 设置里填入".into());
    }
    state
        .orchestrator
        .start(app, meeting_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn stop_meeting(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    state.orchestrator.stop().await.map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn trigger_suggestion(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    if !crate::config::keys_configured() {
        return Err("API key 未配置,请先在 ⚙️ 设置里填入".into());
    }
    state
        .orchestrator
        .trigger_suggestion(app)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn set_suggestions_enabled(
    enabled: bool,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<(), String> {
    if enabled {
        state
            .orchestrator
            .resume_suggestions(app)
            .await
            .map_err(|e| e.to_string())
    } else {
        state
            .orchestrator
            .pause_suggestions()
            .await
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn translate_text(
    text: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<String, String> {
    if !crate::config::keys_configured() {
        return Err("API key 未配置,请先在 ⚙️ 设置里填入".into());
    }
    let llm = state.orchestrator.llm();

    let messages = vec![
        Message::system("You are a translator. Translate the user's text to natural, fluent Chinese. Output ONLY the Chinese translation — no explanation, no quotes, no preamble."),
        Message::user(text),
    ];

    let (tx, mut rx) = mpsc::channel::<String>(64);
    let llm_task = tokio::spawn(async move { llm.stream(messages, tx).await });

    let mut result = String::new();
    while let Some(tok) = rx.recv().await {
        result.push_str(&tok);
    }

    llm_task
        .await
        .map_err(|e| format!("translate join failed: {e}"))?
        .map_err(|e| format!("translate failed: {e}"))?;

    Ok(result.trim().to_string())
}

#[tauri::command]
pub async fn generate_minutes(
    meeting_id: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<String, String> {
    if !crate::config::keys_configured() {
        return Err("API key 未配置,请先在 ⚙️ 设置里填入".into());
    }
    let db = state.orchestrator.db();
    let llm = state.orchestrator.llm();
    let generator = MinutesGenerator::new(db, llm);

    let (tx, mut rx) = mpsc::channel::<String>(256);

    // Spawn token forwarder
    let app_for_recv = app.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(tok) = rx.recv().await {
            let _ = app_for_recv.emit("minutes_token", tok);
        }
    });

    let result = generator.generate(&meeting_id, tx).await;
    let _ = recv_task.await;

    match result {
        Ok(markdown) => {
            let _ = app.emit("minutes_complete", &markdown);
            Ok(markdown)
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = app.emit("minutes_error", &msg);
            Err(msg)
        }
    }
}

#[tauri::command]
pub async fn list_supported_files(folder: String) -> std::result::Result<Vec<String>, String> {
    use std::path::Path;
    let path = Path::new(&folder);
    if !path.is_dir() {
        return Err(format!("not a directory: {folder}"));
    }
    let mut files = Vec::new();
    let entries = std::fs::read_dir(path).map_err(|e| format!("read_dir: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("entry: {e}"))?;
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "pdf" | "docx" | "md" | "txt") {
            if let Some(s) = p.to_str() {
                files.push(s.to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

#[tauri::command]
pub async fn list_meetings(
    state: tauri::State<'_, AppState>,
) -> std::result::Result<Vec<MeetingSummary>, String> {
    let db = state.orchestrator.db();
    let conn = db.conn();
    let mut stmt = conn
        .prepare(
            "SELECT
                m.id, m.name, m.project_ref, m.purpose, m.started_at, m.ended_at,
                (SELECT COUNT(*) FROM transcripts t WHERE t.meeting_id = m.id AND t.is_final = 1) AS transcript_count,
                (SELECT COUNT(*) FROM suggestions s WHERE s.meeting_id = m.id) AS suggestion_count,
                (SELECT COUNT(*) FROM minutes mn WHERE mn.meeting_id = m.id) AS minutes_count
             FROM meetings m
             ORDER BY m.started_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |r| {
            let started_at: i64 = r.get(4)?;
            let ended_at: Option<i64> = r.get(5)?;
            let duration_ms = ended_at.map(|e| e - started_at);
            let minutes_count: i64 = r.get(8)?;
            Ok(MeetingSummary {
                id: r.get(0)?,
                name: r.get(1)?,
                project_ref: r.get(2)?,
                purpose: r.get(3)?,
                started_at,
                ended_at,
                duration_ms,
                transcript_count: r.get(6)?,
                suggestion_count: r.get(7)?,
                has_minutes: minutes_count > 0,
            })
        })
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

#[tauri::command]
pub async fn get_meeting_detail(
    meeting_id: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<MeetingDetail, String> {
    let db = state.orchestrator.db();
    let conn = db.conn();

    let meeting: Meeting = conn
        .query_row(
            "SELECT id, name, project_ref, purpose, participants, started_at, ended_at, audio_path, metadata, focus_points FROM meetings WHERE id = ?",
            [&meeting_id],
            |r| {
                Ok(Meeting {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    project_ref: r.get(2)?,
                    purpose: r.get(3)?,
                    participants: r.get(4)?,
                    started_at: r.get(5)?,
                    ended_at: r.get(6)?,
                    audio_path: r.get(7)?,
                    metadata: r.get(8)?,
                    focus_points: r.get(9)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let transcripts: Vec<TranscriptRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, meeting_id, speaker, text, start_ms, end_ms, is_final FROM transcripts WHERE meeting_id = ? AND is_final = 1 ORDER BY start_ms",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&meeting_id], |r| {
                Ok(TranscriptRow {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    speaker: r.get(2)?,
                    text: r.get(3)?,
                    start_ms: r.get(4)?,
                    end_ms: r.get(5)?,
                    is_final: r.get::<_, i64>(6)? != 0,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row.map_err(|e| e.to_string())?);
        }
        v
    };

    let suggestions: Vec<SuggestionRow> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, meeting_id, triggered_at, trigger_type, style, content, user_action FROM suggestions WHERE meeting_id = ? ORDER BY triggered_at",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&meeting_id], |r| {
                Ok(SuggestionRow {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    triggered_at: r.get(2)?,
                    trigger_type: r.get(3)?,
                    style: r.get(4)?,
                    content: r.get(5)?,
                    user_action: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row.map_err(|e| e.to_string())?);
        }
        v
    };

    let (latest_minutes_md, latest_minutes_version) = conn
        .query_row(
            "SELECT markdown, version FROM minutes WHERE meeting_id = ? ORDER BY version DESC LIMIT 1",
            [&meeting_id],
            |r| Ok::<(String, i64), rusqlite::Error>((r.get(0)?, r.get(1)?)),
        )
        .map(|(md, v)| (Some(md), Some(v)))
        .unwrap_or((None, None));

    Ok(MeetingDetail {
        meeting,
        transcripts,
        suggestions,
        latest_minutes_md,
        latest_minutes_version,
    })
}

#[derive(Serialize)]
pub struct KeyStatus {
    pub aliyun_set: bool,
    pub minimax_set: bool,
}

#[tauri::command]
pub async fn get_api_key_status() -> std::result::Result<KeyStatus, String> {
    Ok(KeyStatus {
        aliyun_set: crate::keychain::get("ALIYUN_API_KEY")
            .ok()
            .flatten()
            .is_some()
            || std::env::var("ALIYUN_API_KEY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
        minimax_set: crate::keychain::get("MINIMAX_API_KEY")
            .ok()
            .flatten()
            .is_some()
            || std::env::var("MINIMAX_API_KEY")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false),
    })
}

#[tauri::command]
pub async fn save_api_keys(
    aliyun: String,
    minimax: String,
    state: tauri::State<'_, AppState>,
) -> std::result::Result<(), String> {
    crate::config::save_keys(&aliyun, &minimax).map_err(|e| e.to_string())?;
    if let Ok(Some(config)) = crate::config::Config::load() {
        state.orchestrator.reconfigure(&config);
    }
    Ok(())
}

#[tauri::command]
pub async fn test_aliyun_key(key: String) -> std::result::Result<(), String> {
    let key = key.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if key.is_empty() {
        return Err("Key 为空".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("client: {e}"))?;

    let resp = client
        .post("https://dashscope.aliyuncs.com/api/v1/services/embeddings/text-embedding/text-embedding")
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "text-embedding-v3",
            "input": { "texts": ["hi"] },
            "parameters": { "dimension": 1024 }
        }))
        .send()
        .await
        .map_err(|e| format!("网络错误: {e}"))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if status.is_success() {
        // Verify response has embeddings (some 200 OK responses still indicate model issues)
        if body.contains("\"embeddings\"") {
            Ok(())
        } else {
            Err(format!("响应异常:{}", body.chars().take(200).collect::<String>()))
        }
    } else if status.as_u16() == 401 {
        Err("Key 无效(401 Unauthorized) — 请检查阿里 DashScope key 是否复制完整".into())
    } else if status.as_u16() == 403 {
        Err("权限不足(403) — 可能 text-embedding-v3 没在百炼开通".into())
    } else {
        Err(format!("HTTP {status}: {}", body.chars().take(200).collect::<String>()))
    }
}

#[tauri::command]
pub async fn test_minimax_key(key: String) -> std::result::Result<(), String> {
    let key = key.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if key.is_empty() {
        return Err("Key 为空".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("client: {e}"))?;

    let resp = client
        .post("https://api.minimaxi.com/v1/text/chatcompletion_v2")
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "MiniMax-M2.7-highspeed",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": false
        }))
        .send()
        .await
        .map_err(|e| format!("网络错误: {e}"))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.as_u16() == 401 {
        return Err("Key 无效(401) — 请检查 MiniMax key 是否复制完整".into());
    }
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", body.chars().take(300).collect::<String>()));
    }

    // MiniMax returns 200 even for plan-not-support errors; check base_resp.status_code
    let parsed: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return Err(format!("响应解析失败: {e}")),
    };

    let base_status = parsed.get("base_resp")
        .and_then(|b| b.get("status_code"))
        .and_then(|c| c.as_i64())
        .unwrap_or(-1);

    if base_status == 0 {
        Ok(())
    } else if base_status == 2061 {
        Err("Token plan 不支持 MiniMax-M2.7-highspeed — 去 https://platform.minimaxi.com 开通这个模型".into())
    } else {
        let msg = parsed.get("base_resp")
            .and_then(|b| b.get("status_msg"))
            .and_then(|m| m.as_str())
            .unwrap_or("(no msg)");
        Err(format!("MiniMax 错误 [{base_status}]: {msg}"))
    }
}
