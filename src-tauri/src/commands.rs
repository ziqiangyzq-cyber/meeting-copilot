use crate::config::Config;
use crate::orchestrator::Orchestrator;
use crate::rag::ingest;
use rusqlite::params;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use uuid::Uuid;

pub struct AppState {
    pub orchestrator: Arc<Orchestrator>,
}

#[tauri::command]
pub async fn create_meeting(
    name: String,
    project_ref: Option<String>,
    purpose: Option<String>,
    participants: Option<String>,
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
        "INSERT INTO meetings (id, name, project_ref, purpose, participants, started_at) VALUES (?, ?, ?, ?, ?, ?)",
        params![meeting_id, name, project_ref, purpose, participants, now],
    )
    .map_err(|e| e.to_string())?;

    Ok(meeting_id)
}

#[tauri::command]
pub async fn ingest_material(
    meeting_id: String,
    file_path: String,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> std::result::Result<String, String> {
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
    let config = Config::from_env().map_err(|e| e.to_string())?;
    state
        .orchestrator
        .start(&config, app, meeting_id)
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
    state
        .orchestrator
        .trigger_suggestion(app)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
