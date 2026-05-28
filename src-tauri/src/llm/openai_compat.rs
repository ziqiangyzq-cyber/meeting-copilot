use crate::error::{AppError, Result};
use crate::llm::{LLMClient, Message};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

const DEFAULT_MAX_TOKENS: u32 = 1024;

pub struct OpenAICompatClient {
    base_url: String,
    api_key: String,
    model: String,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatReq<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatChunk {
    #[serde(default)]
    choices: Vec<ChunkChoice>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    #[serde(default)]
    delta: ChunkDelta,
    #[serde(default)]
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
}

impl OpenAICompatClient {
    pub fn new(base_url: String, api_key: String, model: String) -> Self {
        // Normalize base_url: strip trailing slash
        let base_url = base_url.trim_end_matches('/').to_string();
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client build");
        Self {
            base_url,
            api_key,
            model,
            client,
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

#[async_trait]
impl LLMClient for OpenAICompatClient {
    async fn stream(&self, messages: Vec<Message>, out: mpsc::Sender<String>) -> Result<()> {
        let body = ChatReq {
            model: &self.model,
            messages: &messages,
            stream: true,
            max_tokens: DEFAULT_MAX_TOKENS,
        };

        tracing::info!(
            "openai-compat request: url={} model={} messages={}",
            self.endpoint(),
            self.model,
            messages.len(),
        );

        let resp = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("openai-compat send failed: {:#?}", e);
                AppError::Asr(format!("LLM request failed: {e}"))
            })?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Asr(format!("LLM HTTP {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();

        while let Some(item) = stream.next().await {
            let bytes = item.map_err(|e| AppError::Asr(format!("LLM stream read: {e}")))?;
            buf.extend_from_slice(&bytes);

            // Split on \n\n event boundaries
            loop {
                let Some(pos) = find_subslice(&buf, b"\n\n") else {
                    break;
                };
                let event_bytes = buf.drain(..pos + 2).collect::<Vec<u8>>();
                let event_str = match std::str::from_utf8(&event_bytes[..event_bytes.len() - 2]) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("non-utf8 event skipped: {e}");
                        continue;
                    }
                };
                for line in event_str.lines() {
                    let line = line.trim_start();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let payload = line.trim_start_matches("data:").trim();
                    if payload == "[DONE]" {
                        return Ok(());
                    }
                    if payload.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<ChatChunk>(payload) {
                        Ok(chunk) => {
                            if let Some(choice) = chunk.choices.first() {
                                if let Some(content) = &choice.delta.content {
                                    if !content.is_empty() {
                                        if out.send(content.clone()).await.is_err() {
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("parse chunk failed: {e}, raw: {payload}");
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
