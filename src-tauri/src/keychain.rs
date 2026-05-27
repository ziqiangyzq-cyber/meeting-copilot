use crate::error::{AppError, Result};

const SERVICE: &str = "com.efc.meeting-copilot";

pub fn get(key: &str) -> Result<Option<String>> {
    let entry = keyring::Entry::new(SERVICE, key)
        .map_err(|e| AppError::Config(format!("keychain entry: {e}")))?;
    match entry.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Config(format!("keychain get: {e}"))),
    }
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, key)
        .map_err(|e| AppError::Config(format!("keychain entry: {e}")))?;
    entry
        .set_password(value)
        .map_err(|e| AppError::Config(format!("keychain set: {e}")))?;
    Ok(())
}
