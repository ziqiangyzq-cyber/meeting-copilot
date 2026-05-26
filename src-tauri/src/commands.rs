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

#[tauri::command]
pub async fn show_floating(app: tauri::AppHandle) -> std::result::Result<(), String> {
    use tauri::Manager;
    let win = app
        .get_webview_window("floating")
        .ok_or_else(|| "floating window not found".to_string())?;

    // Position to bottom-right of primary monitor before showing
    if let Some(monitor) = win.current_monitor().map_err(|e| e.to_string())? {
        let size = win.outer_size().map_err(|e| e.to_string())?;
        let mon = monitor.size();
        let scale = monitor.scale_factor();
        let margin = (20.0 * scale) as i32;
        let x = mon.width as i32 - size.width as i32 - margin;
        let y = mon.height as i32 - size.height as i32 - margin * 4; // higher margin from bottom (avoid dock)
        win.set_position(tauri::PhysicalPosition { x, y }).ok();
    }

    win.show().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn hide_floating(app: tauri::AppHandle) -> std::result::Result<(), String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("floating") {
        win.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}
