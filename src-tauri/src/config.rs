use crate::error::{AppError, Result};

pub struct Config {
    pub aliyun_api_key: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let aliyun_api_key = std::env::var("ALIYUN_API_KEY")
            .map_err(|_| AppError::Config("ALIYUN_API_KEY env var not set".into()))?;
        Ok(Self { aliyun_api_key })
    }
}
