use crate::error::{AppError, Result};

const SERVICE: &str = "com.efc.meeting-copilot";

pub fn get(key: &str) -> Result<Option<String>> {
    let entry = keyring::Entry::new(SERVICE, key).map_err(|e| {
        tracing::warn!("keychain::get entry creation failed for {key}: {e}");
        AppError::Config(format!("keychain entry: {e}"))
    })?;
    match entry.get_password() {
        Ok(s) => {
            tracing::info!("keychain::get {key}: found ({} chars)", s.len());
            Ok(Some(s))
        }
        Err(keyring::Error::NoEntry) => {
            tracing::info!("keychain::get {key}: NoEntry");
            Ok(None)
        }
        Err(e) => {
            tracing::warn!("keychain::get {key}: error: {e}");
            Err(AppError::Config(format!("keychain get: {e}")))
        }
    }
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, key).map_err(|e| {
        tracing::warn!("keychain::set entry creation failed for {key}: {e}");
        AppError::Config(format!("keychain entry: {e}"))
    })?;
    entry.set_password(value).map_err(|e| {
        tracing::warn!("keychain::set {key} write failed: {e}");
        AppError::Config(format!("keychain set: {e}"))
    })?;
    tracing::info!("keychain::set {key}: written ({} chars)", value.len());
    Ok(())
}
