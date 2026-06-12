use crate::error::{AppError, Result};
use std::collections::HashMap;
use std::path::PathBuf;

// Historical bundle identifier — kept for backward compatibility with existing installs.
// Does not imply any organizational affiliation.
const APP_DIR_NAME: &str = "com.efc.meeting-copilot";
const KEYS_FILE: &str = "keys.json";

fn keys_dir() -> Result<PathBuf> {
    // Resolve the per-user data dir matching Tauri's app_data_dir for the same
    // identifier on each platform, then create our app subfolder under it:
    //   macOS:   ~/Library/Application Support/com.efc.meeting-copilot/
    //   Windows: %APPDATA%\com.efc.meeting-copilot\   (RoamingAppData)
    //   Linux:   $XDG_DATA_HOME|~/.local/share/com.efc.meeting-copilot/
    let dir = base_data_dir()?.join(APP_DIR_NAME);
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Config(format!("create keys dir: {e}")))?;
    Ok(dir)
}

#[cfg(target_os = "macos")]
fn base_data_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME")
        .map_err(|_| AppError::Config("HOME env var not set".into()))?;
    Ok(PathBuf::from(home).join("Library/Application Support"))
}

#[cfg(target_os = "windows")]
fn base_data_dir() -> Result<PathBuf> {
    // %APPDATA% = C:\Users\<user>\AppData\Roaming (Tauri v2 app_data_dir base on Windows)
    let appdata = std::env::var("APPDATA")
        .map_err(|_| AppError::Config("APPDATA env var not set".into()))?;
    Ok(PathBuf::from(appdata))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn base_data_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.trim().is_empty() {
            return Ok(PathBuf::from(xdg));
        }
    }
    let home = std::env::var("HOME")
        .map_err(|_| AppError::Config("HOME env var not set".into()))?;
    Ok(PathBuf::from(home).join(".local/share"))
}

fn keys_file_path() -> Result<PathBuf> {
    Ok(keys_dir()?.join(KEYS_FILE))
}

fn load_all() -> HashMap<String, String> {
    let Ok(path) = keys_file_path() else { return HashMap::new() };
    let Ok(content) = std::fs::read_to_string(&path) else { return HashMap::new() };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_all(map: &HashMap<String, String>) -> Result<()> {
    let path = keys_file_path()?;
    let content = serde_json::to_string_pretty(map)
        .map_err(|e| AppError::Config(format!("serialize keys: {e}")))?;
    std::fs::write(&path, content)
        .map_err(|e| AppError::Config(format!("write keys file: {e}")))?;
    // Owner-only permissions (rw-------)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&path) {
            let mut perms = metadata.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(&path, perms);
        }
    }
    tracing::info!("keys file written: {}", path.display());
    Ok(())
}

pub fn get(key: &str) -> Result<Option<String>> {
    let map = load_all();
    let value = map.get(key).cloned().filter(|s| !s.trim().is_empty());
    tracing::info!(
        "key_store::get {key}: {}",
        if value.is_some() { "found" } else { "missing" }
    );
    Ok(value)
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let mut map = load_all();
    map.insert(key.to_string(), value.to_string());
    save_all(&map)?;
    tracing::info!("key_store::set {key}: written ({} chars)", value.len());
    Ok(())
}
