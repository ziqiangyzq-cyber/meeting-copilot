mod asr;
mod audio_pump;
mod commands;
mod config;
mod db;
mod error;
mod llm;
mod orchestrator;
mod rag;
mod suggestion;

use commands::{
    create_meeting, ingest_material, start_meeting, stop_meeting, trigger_suggestion, AppState,
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
        .setup(|app| {
            let config = Config::from_env()
                .expect("missing env vars (ALIYUN_API_KEY, MINIMAX_API_KEY)");

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
            trigger_suggestion
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
