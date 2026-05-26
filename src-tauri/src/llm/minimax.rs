use crate::error::{AppError, Result};
use crate::llm::{LLMClient, Message};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// Pick the working endpoint. Domestic first, international as fallback for keys
// that route there. Override via MINIMAX_BASE_URL env if needed.
const URL_DOMESTIC: &str = "https://api.minimaxi.com/v1/text/chatcompletion_v2";
const URL_INTL: &str = "https://api.minimax.io/v1/text/chatcompletion_v2";
// MiniMax-M2.7-highspeed: reasoning model variant accessible under the
// current token plan (abab6.5-chat / M1 / M2 base return 2061 "plan not
// support", verified 2026-05-26). Highspeed variant has lower latency than
// stock M2 while keeping reasoning quality. Emits `reasoning_content` first
// then `content` — we only forward `delta.content`, reasoning is filtered.
const DEFAULT_MODEL: &str = "MiniMax-M2.7-highspeed";
// Reasoning model needs headroom for the hidden chain-of-thought plus the
// visible answer. 1024 leaves enough budget for a short Chinese answer.
const DEFAULT_MAX_TOKENS: u32 = 1024;

pub struct MiniMaxClient {
    api_key: String,
    url: String,
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

impl MiniMaxClient {
    /// Create a client. Uses domestic endpoint by default. To override, pass a base URL.
    pub fn new(api_key: String) -> Self {
        let url =
            std::env::var("MINIMAX_BASE_URL").unwrap_or_else(|_| URL_DOMESTIC.to_string());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("reqwest client build");
        Self {
            api_key,
            url,
            model: DEFAULT_MODEL.into(),
            client,
        }
    }

    /// Try the international endpoint instead. Useful as a fallback.
    pub fn with_international(mut self) -> Self {
        self.url = URL_INTL.into();
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[async_trait]
impl LLMClient for MiniMaxClient {
    async fn stream(&self, messages: Vec<Message>, out: mpsc::Sender<String>) -> Result<()> {
        let body = ChatReq {
            model: &self.model,
            messages: &messages,
            stream: true,
            max_tokens: DEFAULT_MAX_TOKENS,
        };

        let resp = self
            .client
            .post(&self.url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Asr(format!("minimax request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(AppError::Asr(format!("minimax HTTP {status}: {text}")));
        }

        let mut stream = resp.bytes_stream();
        // Use Vec<u8> to safely buffer raw bytes across reads (UTF-8 chars can be split).
        let mut buf: Vec<u8> = Vec::new();

        while let Some(item) = stream.next().await {
            let bytes = item
                .map_err(|e| AppError::Asr(format!("minimax stream read: {e}")))?;
            buf.extend_from_slice(&bytes);

            // SSE events are separated by \n\n. Find complete events and process.
            loop {
                let Some(pos) = find_subslice(&buf, b"\n\n") else {
                    break;
                };
                let event_bytes = buf.drain(..pos + 2).collect::<Vec<u8>>();
                // Drop trailing \n\n for parsing
                let event_str = match std::str::from_utf8(&event_bytes[..event_bytes.len() - 2]) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::warn!("minimax non-utf8 event skipped: {e}");
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
                                            // Receiver dropped — stop
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("minimax parse chunk failed: {e}, raw: {payload}");
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
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Message;
    use tokio::sync::mpsc;

    #[tokio::test]
    #[ignore = "requires MINIMAX_API_KEY env and network"]
    async fn stream_chinese_response() {
        let key = std::env::var("MINIMAX_API_KEY").expect("MINIMAX_API_KEY not set");

        let client = MiniMaxClient::new(key.clone());
        let (tx, mut rx) = mpsc::channel::<String>(64);

        let messages = vec![
            Message::system("你是一个友好的中文助手。回答简短,不超过 30 字。"),
            Message::user("用一句话介绍 EFC 创羿幕墙顾问公司。"),
        ];

        let handle = tokio::spawn(async move { client.stream(messages, tx).await });

        let mut accumulated = String::new();
        while let Some(tok) = rx.recv().await {
            print!("{tok}");
            std::io::Write::flush(&mut std::io::stdout()).ok();
            accumulated.push_str(&tok);
        }
        println!();

        let result = handle.await.unwrap();

        if let Err(e) = &result {
            // If domestic endpoint fails with 401/403, try international
            if format!("{e}").contains("401") || format!("{e}").contains("403") {
                println!("\n[warn] domestic endpoint rejected key; retrying with international...");

                let client = MiniMaxClient::new(key).with_international();
                let (tx, mut rx) = mpsc::channel::<String>(64);
                let messages = vec![
                    Message::system("你是一个友好的中文助手。回答简短,不超过 30 字。"),
                    Message::user("用一句话介绍 EFC 创羿幕墙顾问公司。"),
                ];
                let handle = tokio::spawn(async move { client.stream(messages, tx).await });
                let mut accumulated2 = String::new();
                while let Some(tok) = rx.recv().await {
                    print!("{tok}");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                    accumulated2.push_str(&tok);
                }
                println!();
                handle
                    .await
                    .unwrap()
                    .expect("international endpoint also failed");
                assert!(!accumulated2.is_empty(), "international response empty");
                println!(
                    "\n=== INTERNATIONAL OK ===\nResponse length: {} chars\n",
                    accumulated2.chars().count()
                );
                return;
            }
            panic!("stream failed: {e}");
        }

        assert!(!accumulated.is_empty(), "response should not be empty");
        // Sanity: should contain at least one CJK char
        assert!(
            accumulated
                .chars()
                .any(|c| c >= '\u{4e00}' && c <= '\u{9fff}'),
            "response should contain Chinese chars, got: {accumulated}"
        );
        println!(
            "\n=== DOMESTIC OK ===\nResponse length: {} chars\n",
            accumulated.chars().count()
        );
    }
}
