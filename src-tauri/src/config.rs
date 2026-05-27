use crate::error::Result;
use crate::keychain;

#[derive(Clone)]
pub struct Config {
    pub aliyun_api_key: String,
    pub minimax_api_key: String,
}

const ALIYUN_KEY_NAME: &str = "ALIYUN_API_KEY";
const MINIMAX_KEY_NAME: &str = "MINIMAX_API_KEY";

fn sanitize(s: String) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn load_one(name: &str) -> Result<Option<String>> {
    // Priority: env var (dev override) > Keychain
    if let Ok(v) = std::env::var(name) {
        let cleaned = sanitize(v);
        if !cleaned.is_empty() {
            return Ok(Some(cleaned));
        }
    }
    Ok(keychain::get(name)?.map(sanitize).filter(|s| !s.is_empty()))
}

impl Config {
    /// Load both keys. Returns None if not fully configured.
    pub fn load() -> Result<Option<Self>> {
        let aliyun = load_one(ALIYUN_KEY_NAME)?;
        let minimax = load_one(MINIMAX_KEY_NAME)?;
        match (aliyun, minimax) {
            (Some(a), Some(m)) => Ok(Some(Self {
                aliyun_api_key: a,
                minimax_api_key: m,
            })),
            _ => Ok(None),
        }
    }
}

pub fn save_keys(aliyun: &str, minimax: &str) -> Result<()> {
    keychain::set(ALIYUN_KEY_NAME, &sanitize(aliyun.to_string()))?;
    keychain::set(MINIMAX_KEY_NAME, &sanitize(minimax.to_string()))?;
    Ok(())
}

pub fn keys_configured() -> bool {
    matches!(load_one(ALIYUN_KEY_NAME), Ok(Some(_)))
        && matches!(load_one(MINIMAX_KEY_NAME), Ok(Some(_)))
}
