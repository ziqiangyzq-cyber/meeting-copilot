use crate::asr::aliyun_paraformer::TranscriptEvent;
use crate::asr::AudioSource;
use crate::db::Db;
use crate::error::Result;
use crate::llm::{LLMClient, Message};
use crate::rag::{embedding::EmbeddingClient, retrieve};
use crate::suggestion::prompt::{system_prompt, user_prompt, MeetingMeta};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

const BUFFER_WINDOW_SECS: u64 = 120;
const PROMPT_WINDOW_SECS: u64 = 90;
const QUERY_WINDOW_SECS: u64 = 30;
const RAG_TOP_K: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerType {
    Auto,
    Manual,
}

#[derive(Debug, Clone)]
struct BufferedEvent {
    received_at: Instant,
    source: AudioSource,
    text: String,
    is_final: bool,
}

#[derive(Default)]
struct TranscriptBuffer {
    events: VecDeque<BufferedEvent>,
}

impl TranscriptBuffer {
    fn push(&mut self, evt: TranscriptEvent) {
        // Drop partial events when a new one from same source arrives
        if !evt.is_final {
            if let Some(last) = self.events.back_mut() {
                if !last.is_final && last.source == evt.source {
                    last.text = evt.text;
                    last.received_at = Instant::now();
                    return;
                }
            }
        }
        self.events.push_back(BufferedEvent {
            received_at: Instant::now(),
            source: evt.source,
            text: evt.text,
            is_final: evt.is_final,
        });
        self.gc();
    }

    fn gc(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(BUFFER_WINDOW_SECS);
        while let Some(front) = self.events.front() {
            if front.received_at < cutoff {
                self.events.pop_front();
            } else {
                break;
            }
        }
    }

    /// Format last `secs` of buffer as a human-readable string with speaker labels.
    fn recent_text(&self, secs: u64) -> String {
        let cutoff = Instant::now() - Duration::from_secs(secs);
        let mut out = String::new();
        for e in &self.events {
            if e.received_at < cutoff {
                continue;
            }
            let label = match e.source {
                AudioSource::System => "对方",
                AudioSource::Mic => "我",
            };
            out.push_str(&format!("{label}: {}\n", e.text));
        }
        out
    }
}

pub struct SuggestionEngine {
    buffer: Arc<Mutex<TranscriptBuffer>>,
    db: Arc<Db>,
    embed: Arc<EmbeddingClient>,
    llm: Arc<dyn LLMClient>,
    meeting_id: String,
}

impl SuggestionEngine {
    pub fn new(
        db: Arc<Db>,
        embed: Arc<EmbeddingClient>,
        llm: Arc<dyn LLMClient>,
        meeting_id: String,
    ) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(TranscriptBuffer::default())),
            db,
            embed,
            llm,
            meeting_id,
        }
    }

    pub async fn push_transcript(&self, evt: TranscriptEvent) {
        self.buffer.lock().await.push(evt);
    }

    /// Re-read meeting metadata (including focus_points) from DB on every call so
    /// mid-meeting edits to focus_points take effect immediately on next generation.
    fn load_meta(&self) -> Result<MeetingMeta> {
        let conn = self.db.conn();
        let meta = conn.query_row(
            "SELECT name, project_ref, purpose, participants, focus_points FROM meetings WHERE id = ?",
            [&self.meeting_id],
            |r| {
                Ok(MeetingMeta {
                    name: r.get(0)?,
                    project_ref: r.get(1)?,
                    purpose: r.get(2)?,
                    participants: r.get(3)?,
                    focus_points: r.get(4)?,
                })
            },
        )?;
        Ok(meta)
    }

    /// Generate one suggestion. Streams tokens to `out`. Returns Ok(()) on
    /// success, Err on RAG/LLM failure. If the buffer has no recent transcript,
    /// returns Ok without emitting anything (no-op).
    pub async fn generate(
        &self,
        _trigger: TriggerType,
        out: mpsc::Sender<String>,
    ) -> Result<()> {
        let recent = self.buffer.lock().await.recent_text(PROMPT_WINDOW_SECS);
        if recent.trim().is_empty() {
            return Ok(());
        }
        tracing::info!("suggestion generate: recent_transcript_chars={}", recent.chars().count());

        let query = self.buffer.lock().await.recent_text(QUERY_WINDOW_SECS);
        let query_str = if query.trim().is_empty() {
            recent.clone()
        } else {
            query
        };

        // Re-read meta from DB so mid-meeting focus_points edits flow through immediately
        let meta = self.load_meta()?;

        // RAG retrieve top-K. If embedding/retrieve fails, fall back to empty chunks
        // (suggestion still gets generated, just without references).
        let chunks = match retrieve::retrieve(
            &self.db,
            &self.embed,
            &self.meeting_id,
            &query_str,
            RAG_TOP_K,
        )
        .await
        {
            Ok(c) => {
                tracing::info!(
                    "RAG retrieve: chunks={} first_distance={}",
                    c.len(),
                    c.first().map(|ch| format!("{:.4}", ch.distance)).unwrap_or_else(|| "no chunks".into()),
                );
                c
            }
            Err(e) => {
                tracing::warn!("RAG retrieve failed, proceeding without context: {e}");
                Vec::new()
            }
        };

        let system = system_prompt();
        let user = user_prompt(&meta, &recent, &chunks);

        let messages = vec![Message::system(system), Message::user(user)];
        tracing::info!("suggestion generate: calling LLM stream, messages={}", messages.len());
        self.llm.stream(messages, out).await
    }

    /// Spawn a background task that calls generate() every `interval` seconds.
    /// Emits "suggestion_token" event for each token, "suggestion_complete" on
    /// completion. Returns the JoinHandle so the caller can abort on shutdown.
    pub fn start_auto_timer(
        self: Arc<Self>,
        interval: Duration,
        app: tauri::AppHandle,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            use tauri::Emitter;
            loop {
                tokio::time::sleep(interval).await;

                let (tx, mut rx) = mpsc::channel::<String>(64);
                let app_for_recv = app.clone();
                let recv_task = tokio::spawn(async move {
                    while let Some(tok) = rx.recv().await {
                        let _ = app_for_recv.emit("suggestion_token", tok);
                    }
                });

                let result = self.generate(TriggerType::Auto, tx).await;
                let _ = recv_task.await;

                if let Err(e) = result {
                    tracing::warn!("auto suggestion failed: {e}");
                    let _ = app.emit("suggestion_error", format!("{e}"));
                } else {
                    let _ = app.emit("suggestion_complete", ());
                }
            }
        })
    }
}
