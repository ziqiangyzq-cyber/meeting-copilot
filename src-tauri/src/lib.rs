mod asr;
mod audio_pump;
mod commands;
mod config;
mod db;
mod error;
mod orchestrator;
mod rag;

use commands::{start_meeting, stop_meeting, AppState};
use orchestrator::Orchestrator;
use std::sync::Arc;

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
        .manage(AppState {
            orchestrator: Arc::new(Orchestrator::new()),
        })
        .invoke_handler(tauri::generate_handler![greet, start_meeting, stop_meeting])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
