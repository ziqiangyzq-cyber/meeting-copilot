pub mod minimax;

use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "system" | "user" | "assistant"
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
}

/// Stream tokens from an LLM. Each token chunk is sent via `out` as it arrives.
/// Returns Ok(()) on clean completion, Err on protocol/network failure.
#[async_trait]
pub trait LLMClient: Send + Sync {
    async fn stream(&self, messages: Vec<Message>, out: mpsc::Sender<String>) -> Result<()>;
}
