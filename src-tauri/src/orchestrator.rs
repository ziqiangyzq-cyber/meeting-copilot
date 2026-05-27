use crate::asr::aliyun_paraformer::{AliyunParaformer, TranscriptEvent};
use crate::asr::{ASRClient, AudioSource as AsrSource};
use crate::audio_pump::{frame::AudioSource as PumpSource, HelperProc};
use crate::config::Config;
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::llm::{minimax::MiniMaxClient, LLMClient};
use crate::rag::embedding::EmbeddingClient;
use crate::suggestion::{SuggestionEngine, TriggerType};
use rusqlite::params;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

const AUTO_SUGGESTION_INTERVAL_SECS: u64 = 20;

pub struct Orchestrator {
    inner: Arc<Mutex<OrchestratorState>>,
    db: Arc<Db>,
    embed: Arc<RwLock<Arc<EmbeddingClient>>>,
    llm: Arc<RwLock<Arc<dyn LLMClient>>>,
    config: Arc<RwLock<Config>>,
}

struct OrchestratorState {
    helper: Option<HelperProc>,
    forward_handle: Option<JoinHandle<()>>,
    transcript_handle: Option<JoinHandle<()>>,
    suggestion_engine: Option<Arc<SuggestionEngine>>,
    suggestion_timer: Option<JoinHandle<()>>,
    current_meeting_id: Option<String>,
}

impl Orchestrator {
    pub fn new(config: &Config, db: Arc<Db>) -> Self {
        let embed = Arc::new(EmbeddingClient::new(config.aliyun_api_key.clone()));
        let llm: Arc<dyn LLMClient> = Arc::new(MiniMaxClient::new(config.minimax_api_key.clone()));
        Self {
            inner: Arc::new(Mutex::new(OrchestratorState {
                helper: None,
                forward_handle: None,
                transcript_handle: None,
                suggestion_engine: None,
                suggestion_timer: None,
                current_meeting_id: None,
            })),
            db,
            embed: Arc::new(RwLock::new(embed)),
            llm: Arc::new(RwLock::new(llm)),
            config: Arc::new(RwLock::new(config.clone())),
        }
    }

    pub fn db(&self) -> Arc<Db> {
        self.db.clone()
    }

    pub fn embed(&self) -> Arc<EmbeddingClient> {
        self.embed.read().unwrap().clone()
    }

    pub fn llm(&self) -> Arc<dyn LLMClient> {
        self.llm.read().unwrap().clone()
    }

    pub fn current_aliyun_key(&self) -> String {
        self.config.read().unwrap().aliyun_api_key.clone()
    }

    pub fn reconfigure(&self, config: &Config) {
        let new_embed = Arc::new(EmbeddingClient::new(config.aliyun_api_key.clone()));
        let new_llm: Arc<dyn LLMClient> =
            Arc::new(MiniMaxClient::new(config.minimax_api_key.clone()));
        *self.embed.write().unwrap() = new_embed;
        *self.llm.write().unwrap() = new_llm;
        *self.config.write().unwrap() = config.clone();
        tracing::info!("orchestrator clients reconfigured");
    }

    /// Start a meeting: spawn AudioHelper, connect ASR, init SuggestionEngine, start auto timer.
    pub async fn start(
        &self,
        app: tauri::AppHandle,
        meeting_id: String,
    ) -> Result<()> {
        let mut state = self.inner.lock().await;

        if state.helper.is_some() {
            return Err(AppError::AudioHelper("already running".into()));
        }

        // 1. Spawn AudioHelper
        let bin_path = locate_helper_binary()?;
        let mut helper = HelperProc::spawn(bin_path).await?;
        helper.send_cmd("start").await?;

        // 2. Connect ASR
        let (transcript_tx, mut transcript_rx) = mpsc::channel::<TranscriptEvent>(64);
        let asr =
            AliyunParaformer::connect(self.current_aliyun_key(), None, transcript_tx).await?;
        let asr = Arc::new(Mutex::new(asr));

        // 3. Build SuggestionEngine (meta re-read from DB on each generate so
        // mid-meeting focus_points edits take effect immediately).
        let engine = Arc::new(SuggestionEngine::new(
            self.db.clone(),
            self.embed(),
            self.llm(),
            meeting_id.clone(),
        ));

        // 4. Pump frames from helper → ASR
        let frames_rx = helper
            .take_frames()
            .ok_or_else(|| AppError::AudioHelper("frames already taken".into()))?;
        let asr_for_pump = asr.clone();
        let forward = tokio::spawn(async move {
            let mut rx = frames_rx;
            while let Some(frame) = rx.recv().await {
                let asr_src = match frame.source {
                    PumpSource::System => AsrSource::System,
                    PumpSource::Mic => AsrSource::Mic,
                };
                let mut a = asr_for_pump.lock().await;
                if let Err(e) = a.push_pcm(asr_src, &frame.pcm).await {
                    tracing::error!("ASR push_pcm failed: {e}");
                    break;
                }
            }
            // Frame channel closed — close ASR streams so finish-task is sent
            let mut a = asr_for_pump.lock().await;
            let _ = a.close().await;
            tracing::info!("frame forwarder ended");
        });

        // 5. Transcript loop: emit to UI + persist final events + push to SuggestionEngine
        let engine_for_transcript = engine.clone();
        let db_for_transcript = self.db.clone();
        let meeting_id_for_transcript = meeting_id.clone();
        let app_for_transcript = app.clone();
        let transcript_loop = tokio::spawn(async move {
            while let Some(evt) = transcript_rx.recv().await {
                // Frontend emit
                let payload = serde_json::json!({
                    "source": match evt.source {
                        AsrSource::System => "system",
                        AsrSource::Mic => "mic",
                    },
                    "text": evt.text,
                    "is_final": evt.is_final,
                    "begin_ms": evt.begin_ms,
                    "end_ms": evt.end_ms,
                });
                if let Err(e) = app_for_transcript.emit("transcript", payload) {
                    tracing::warn!("emit transcript failed: {e}");
                }

                // Persist final transcripts
                if evt.is_final {
                    let conn = db_for_transcript.conn();
                    let speaker = match evt.source {
                        AsrSource::System => "system",
                        AsrSource::Mic => "mic",
                    };
                    if let Err(e) = conn.execute(
                        "INSERT INTO transcripts (meeting_id, speaker, text, start_ms, end_ms, is_final) VALUES (?, ?, ?, ?, ?, 1)",
                        params![
                            meeting_id_for_transcript,
                            speaker,
                            evt.text,
                            evt.begin_ms as i64,
                            evt.end_ms as i64,
                        ],
                    ) {
                        tracing::warn!("persist transcript failed: {e}");
                    }
                }

                // Push to SuggestionEngine buffer
                engine_for_transcript.push_transcript(evt).await;
            }
            tracing::info!("transcript loop ended");
        });

        // 6. Start auto-suggestion timer
        let app_for_timer = app.clone();
        let timer_handle = engine.clone().start_auto_timer(
            Duration::from_secs(AUTO_SUGGESTION_INTERVAL_SECS),
            app_for_timer,
        );

        state.helper = Some(helper);
        state.forward_handle = Some(forward);
        state.transcript_handle = Some(transcript_loop);
        state.suggestion_engine = Some(engine);
        state.suggestion_timer = Some(timer_handle);
        state.current_meeting_id = Some(meeting_id);

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.inner.lock().await;

        // Shutdown helper (sends "stop" cmd + waits for child exit).
        if let Some(helper) = state.helper.take() {
            helper.shutdown().await?;
        }
        if let Some(h) = state.forward_handle.take() {
            h.abort();
        }
        if let Some(h) = state.transcript_handle.take() {
            h.abort();
        }
        if let Some(h) = state.suggestion_timer.take() {
            h.abort();
        }
        state.suggestion_engine = None;

        // Mark meeting ended
        if let Some(meeting_id) = state.current_meeting_id.take() {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            let conn = self.db.conn();
            if let Err(e) = conn.execute(
                "UPDATE meetings SET ended_at = ? WHERE id = ?",
                params![now, meeting_id],
            ) {
                tracing::warn!("update meeting ended_at failed: {e}");
            }
        }

        Ok(())
    }

    /// Pause auto-suggestion timer. Engine + buffer remain so resume preserves context.
    pub async fn pause_suggestions(&self) -> Result<()> {
        let mut state = self.inner.lock().await;
        if let Some(h) = state.suggestion_timer.take() {
            h.abort();
            tracing::info!("suggestion timer paused");
        }
        Ok(())
    }

    /// Resume auto-suggestion timer. No-op if no active meeting or already running.
    pub async fn resume_suggestions(&self, app: tauri::AppHandle) -> Result<()> {
        let mut state = self.inner.lock().await;
        if state.suggestion_timer.is_some() {
            return Ok(()); // already running
        }
        let Some(engine) = state.suggestion_engine.clone() else {
            return Ok(()); // no active meeting
        };
        let timer_handle = engine.start_auto_timer(
            Duration::from_secs(AUTO_SUGGESTION_INTERVAL_SECS),
            app,
        );
        state.suggestion_timer = Some(timer_handle);
        tracing::info!("suggestion timer resumed");
        Ok(())
    }

    /// Manually trigger a suggestion. Returns Err if no meeting is active.
    pub async fn trigger_suggestion(&self, app: tauri::AppHandle) -> Result<()> {
        let engine = {
            let state = self.inner.lock().await;
            state
                .suggestion_engine
                .clone()
                .ok_or_else(|| AppError::Asr("no active meeting".into()))?
        };

        let (tx, mut rx) = mpsc::channel::<String>(64);
        let app_for_recv = app.clone();
        let recv_task = tokio::spawn(async move {
            while let Some(tok) = rx.recv().await {
                let _ = app_for_recv.emit("suggestion_token", tok);
            }
        });

        let result = engine.generate(TriggerType::Manual, tx).await;
        let _ = recv_task.await;

        if let Err(e) = result {
            let _ = app.emit("suggestion_error", format!("{e}"));
            return Err(e);
        }
        let _ = app.emit("suggestion_complete", ());
        Ok(())
    }
}

fn locate_helper_binary() -> Result<PathBuf> {
    // Priority:
    // 1. AUDIO_HELPER_PATH env (override for dev / debugging)
    // 2. Dev paths relative to where the Tauri binary runs

    if let Ok(p) = std::env::var("AUDIO_HELPER_PATH") {
        return Ok(PathBuf::from(p));
    }

    let candidates = [
        "audio-helper/.build/release/AudioHelper",
        "../audio-helper/.build/release/AudioHelper",
    ];
    for path in candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    Err(AppError::AudioHelper(
        "AudioHelper binary not found; set AUDIO_HELPER_PATH env or build audio-helper first (cd audio-helper && swift build -c release)".into(),
    ))
}
