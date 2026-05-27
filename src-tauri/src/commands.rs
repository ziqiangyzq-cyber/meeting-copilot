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

    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten());

    if let Some(monitor) = monitor {
        let size = win.outer_size().map_err(|e| e.to_string())?;
        let mon_size = monitor.size();
        let mon_pos = monitor.position();
        let scale = monitor.scale_factor();
        let margin = (20.0 * scale) as i32;
        // Right edge, vertically centered
        let x = mon_pos.x + mon_size.width as i32 - size.width as i32 - margin;
        let y = mon_pos.y + (mon_size.height as i32 - size.height as i32) / 2;
        tracing::info!(
            "show_floating: positioning to ({}, {}) right-middle on monitor {}x{} @ ({}, {})",
            x,
            y,
            mon_size.width,
            mon_size.height,
            mon_pos.x,
            mon_pos.y
        );
        win.set_position(tauri::PhysicalPosition { x, y })
            .map_err(|e| format!("set_position failed: {e}"))?;
    } else {
        tracing::warn!("show_floating: no monitor found, using fallback (1400, 300)");
        win.set_position(tauri::PhysicalPosition {
            x: 1400_i32,
            y: 300_i32,
        })
        .map_err(|e| format!("set_position fallback failed: {e}"))?;
    }

    win.show().map_err(|e| format!("show failed: {e}"))?;
    tracing::info!("show_floating: window shown");
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

#[tauri::command]
pub async fn collapse_floating(app: tauri::AppHandle) -> std::result::Result<(), String> {
    use tauri::Manager;
    let win = app
        .get_webview_window("floating")
        .ok_or_else(|| "floating window not found".to_string())?;
    win.set_size(tauri::PhysicalSize {
        width: 80_u32,
        height: 80_u32,
    })
    .map_err(|e| format!("set_size failed: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn expand_floating(app: tauri::AppHandle) -> std::result::Result<(), String> {
    use tauri::Manager;
    let win = app
        .get_webview_window("floating")
        .ok_or_else(|| "floating window not found".to_string())?;
    win.set_size(tauri::PhysicalSize {
        width: 220_u32,
        height: 400_u32,
    })
    .map_err(|e| format!("set_size failed: {e}"))?;
    Ok(())
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
