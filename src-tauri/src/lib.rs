mod asr;
mod audio_pump;
mod commands;
mod config;
mod db;
mod error;
mod keychain;
mod llm;
mod minutes;
mod orchestrator;
mod rag;
mod suggestion;

use commands::{
    create_meeting, delete_meeting, generate_minutes, get_api_key_status, get_meeting_detail,
    ingest_material, list_meetings, list_supported_files, restart_mic, save_api_keys,
    set_suggestions_enabled, start_meeting, stop_meeting, test_aliyun_key, test_minimax_key,
    translate_text, trigger_suggestion, update_focus_points, AppState,
};
use config::Config;
use db::Db;
use orchestrator::Orchestrator;
use std::sync::Arc;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let config = Config::load().unwrap_or(None).unwrap_or_else(|| Config {
                aliyun_api_key: String::new(),
                minimax_api_key: String::new(),
            });

            tracing::info!(
                "startup config: aliyun_set={} (len={}), minimax_set={} (len={})",
                !config.aliyun_api_key.is_empty(),
                config.aliyun_api_key.len(),
                !config.minimax_api_key.is_empty(),
                config.minimax_api_key.len(),
            );

            // DB at platform-appropriate data dir
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("no app data dir available");
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("meeting-copilot.sqlite");
            let db = Arc::new(Db::open(&db_path).expect("db open failed"));

            let orch = Arc::new(Orchestrator::new(&config, db));
            app.manage(AppState { orchestrator: orch });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            create_meeting,
            ingest_material,
            start_meeting,
            stop_meeting,
            trigger_suggestion,
            restart_mic,
            set_suggestions_enabled,
            translate_text,
            list_supported_files,
            generate_minutes,
            list_meetings,
            get_meeting_detail,
            delete_meeting,
            update_focus_points,
            get_api_key_status,
            save_api_keys,
            test_aliyun_key,
            test_minimax_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
