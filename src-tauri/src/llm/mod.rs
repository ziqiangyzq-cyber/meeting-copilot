pub mod minimax;
pub mod openai_compat;

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

/// Default output budget for short in-meeting tasks (suggestions, translation).
/// Reasoning models spend hidden chain-of-thought from the same budget, so this
/// is NOT enough for long outputs like minutes — use stream_max for those.
pub const DEFAULT_MAX_TOKENS: u32 = 1024;

/// Stream tokens from an LLM. Each token chunk is sent via `out` as it arrives.
/// Returns Ok(()) on clean completion, Err on protocol/network failure.
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// Stream with the default (short-answer) token budget.
    async fn stream(&self, messages: Vec<Message>, out: mpsc::Sender<String>) -> Result<()> {
        self.stream_max(messages, out, DEFAULT_MAX_TOKENS).await
    }

    /// Stream with an explicit max_tokens budget (long outputs like minutes).
    async fn stream_max(
        &self,
        messages: Vec<Message>,
        out: mpsc::Sender<String>,
        max_tokens: u32,
    ) -> Result<()>;
}
