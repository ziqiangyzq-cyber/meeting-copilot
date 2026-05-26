use crate::error::{AppError, Result};

pub struct Config {
    pub aliyun_api_key: String,
    pub minimax_api_key: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let aliyun_api_key = sanitize_key(
            std::env::var("ALIYUN_API_KEY")
                .map_err(|_| AppError::Config("ALIYUN_API_KEY env var not set".into()))?
        );
        let minimax_api_key = sanitize_key(
            std::env::var("MINIMAX_API_KEY")
                .map_err(|_| AppError::Config("MINIMAX_API_KEY env var not set".into()))?
        );
        Ok(Self {
            aliyun_api_key,
            minimax_api_key,
        })
    }
}

fn sanitize_key(raw: String) -> String {
    raw.chars().filter(|c| !c.is_whitespace()).collect()
}
