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
mod templates;

use commands::{
    create_meeting, delete_meeting, export_minutes_docx, generate_minutes, get_api_key_status,
    get_llm_status, get_meeting_detail, get_voice_processing, ingest_material, list_meetings,
    list_supported_files, list_templates, restart_mic, save_aliyun_only, save_api_keys,
    save_minimax_only, save_openai_compat, set_suggestions_enabled, set_voice_processing,
    start_meeting, stop_meeting, test_aliyun_key, test_minimax_key, test_openai_compat,
    translate_text, trigger_suggestion, update_focus_points, update_meeting_notes, AppState,
};
use config::{Config, LlmProvider};
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
                llm_provider: LlmProvider::default(),
                minimax_api_key: String::new(),
                llm_base_url: String::new(),
                llm_model: String::new(),
                llm_api_key: String::new(),
                voice_processing_enabled: true,
            });

            tracing::info!(
                "startup config: aliyun_set={} provider={:?} minimax_set={} openai_compat_set={} (base_url_len={} model_len={} key_len={})",
                !config.aliyun_api_key.is_empty(),
                config.llm_provider,
                !config.minimax_api_key.is_empty(),
                !config.llm_base_url.is_empty()
                    && !config.llm_model.is_empty()
                    && !config.llm_api_key.is_empty(),
                config.llm_base_url.len(),
                config.llm_model.len(),
                config.llm_api_key.len(),
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
            update_meeting_notes,
            export_minutes_docx,
            get_api_key_status,
            save_api_keys,
            test_aliyun_key,
            test_minimax_key,
            get_llm_status,
            save_aliyun_only,
            save_minimax_only,
            save_openai_compat,
            test_openai_compat,
            set_voice_processing,
            get_voice_processing,
            list_templates
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
