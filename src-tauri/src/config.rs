use crate::error::Result;
use crate::keychain;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum LlmProvider {
    #[serde(rename = "minimax")]
    MiniMax,
    #[serde(rename = "openai_compat")]
    OpenAICompat,
}

impl Default for LlmProvider {
    fn default() -> Self {
        LlmProvider::MiniMax
    }
}

#[derive(Clone)]
pub struct Config {
    pub aliyun_api_key: String,
    /// Provider choice
    pub llm_provider: LlmProvider,
    /// MiniMax key (used when provider = MiniMax)
    pub minimax_api_key: String,
    /// OpenAI-compat base URL (e.g., https://api.deepseek.com/v1)
    pub llm_base_url: String,
    /// OpenAI-compat model name (e.g., deepseek-chat)
    pub llm_model: String,
    /// OpenAI-compat API key
    pub llm_api_key: String,
}

const KEY_ALIYUN: &str = "ALIYUN_API_KEY";
const KEY_MINIMAX: &str = "MINIMAX_API_KEY";
const KEY_LLM_PROVIDER: &str = "LLM_PROVIDER";
const KEY_LLM_BASE_URL: &str = "LLM_BASE_URL";
const KEY_LLM_MODEL: &str = "LLM_MODEL";
const KEY_LLM_API_KEY: &str = "LLM_API_KEY";

fn sanitize(s: String) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn load_one(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(name) {
        let cleaned = sanitize(v);
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    }
    keychain::get(name)
        .ok()
        .flatten()
        .map(sanitize)
        .filter(|s| !s.is_empty())
}

fn load_raw(name: &str) -> Option<String> {
    // For non-sensitive values (URL, model name), don't strip internal whitespace
    if let Ok(v) = std::env::var(name) {
        let trimmed = v.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    keychain::get(name)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Config {
    /// Load config. Returns None if not fully configured (missing aliyun or provider-specific keys).
    pub fn load() -> Result<Option<Self>> {
        let Some(aliyun) = load_one(KEY_ALIYUN) else {
            return Ok(None);
        };

        // Determine provider — default to MiniMax for backward compat
        let provider_str = load_raw(KEY_LLM_PROVIDER).unwrap_or_else(|| "minimax".to_string());
        let provider = match provider_str.as_str() {
            "openai_compat" => LlmProvider::OpenAICompat,
            _ => LlmProvider::MiniMax,
        };

        match provider {
            LlmProvider::MiniMax => {
                let Some(minimax_key) = load_one(KEY_MINIMAX) else {
                    return Ok(None);
                };
                Ok(Some(Self {
                    aliyun_api_key: aliyun,
                    llm_provider: provider,
                    minimax_api_key: minimax_key,
                    llm_base_url: String::new(),
                    llm_model: String::new(),
                    llm_api_key: String::new(),
                }))
            }
            LlmProvider::OpenAICompat => {
                let Some(base) = load_raw(KEY_LLM_BASE_URL) else {
                    return Ok(None);
                };
                let Some(model) = load_raw(KEY_LLM_MODEL) else {
                    return Ok(None);
                };
                let Some(key) = load_one(KEY_LLM_API_KEY) else {
                    return Ok(None);
                };
                Ok(Some(Self {
                    aliyun_api_key: aliyun,
                    llm_provider: provider,
                    minimax_api_key: String::new(),
                    llm_base_url: base,
                    llm_model: model,
                    llm_api_key: key,
                }))
            }
        }
    }
}

pub fn save_keys(aliyun: &str, minimax: &str) -> Result<()> {
    keychain::set(KEY_ALIYUN, &sanitize(aliyun.to_string()))?;
    keychain::set(KEY_MINIMAX, &sanitize(minimax.to_string()))?;
    Ok(())
}

pub fn save_aliyun_key(aliyun: &str) -> Result<()> {
    keychain::set(KEY_ALIYUN, &sanitize(aliyun.to_string()))
}

pub fn save_llm_provider(provider: &LlmProvider) -> Result<()> {
    let v = match provider {
        LlmProvider::MiniMax => "minimax",
        LlmProvider::OpenAICompat => "openai_compat",
    };
    keychain::set(KEY_LLM_PROVIDER, v)
}

pub fn save_minimax_key(key: &str) -> Result<()> {
    keychain::set(KEY_MINIMAX, &sanitize(key.to_string()))
}

