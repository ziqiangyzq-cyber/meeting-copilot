use crate::asr::aliyun_paraformer::{AliyunParaformer, TranscriptEvent};
use crate::asr::{ASRClient, AudioSource as AsrSource};
use crate::audio_pump::{frame::AudioSource as PumpSource, HelperProc};
use crate::config::Config;
use crate::error::{AppError, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

pub struct Orchestrator {
    inner: Arc<Mutex<OrchestratorState>>,
}

struct OrchestratorState {
    helper: Option<HelperProc>,
    forward_handle: Option<JoinHandle<()>>,
    transcript_handle: Option<JoinHandle<()>>,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(OrchestratorState {
                helper: None,
                forward_handle: None,
                transcript_handle: None,
            })),
        }
    }

    pub async fn start(&self, config: &Config, app: tauri::AppHandle) -> Result<()> {
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
        let asr = AliyunParaformer::connect(
            config.aliyun_api_key.clone(),
            None,
            transcript_tx,
        )
        .await?;
        let asr = Arc::new(Mutex::new(asr));

        // 3. Pump frames from helper → ASR
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

        // 4. Emit transcripts to frontend
        let transcript_loop = tokio::spawn(async move {
            while let Some(evt) = transcript_rx.recv().await {
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
                if let Err(e) = app.emit("transcript", payload) {
                    tracing::warn!("emit transcript failed: {e}");
                }
            }
            tracing::info!("transcript emit loop ended");
        });

        state.helper = Some(helper);
        state.forward_handle = Some(forward);
        state.transcript_handle = Some(transcript_loop);

        // ASR is kept alive by the spawned forward task via Arc<Mutex<>>.
        // When the forward task ends (frames_rx closed), it calls close() on the ASR,
        // which drops the pcm send channels — the ASR's internal tasks then send
        // finish-task and shut down naturally.

        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut state = self.inner.lock().await;

        // Shutdown helper (sends "stop" cmd + waits for child exit).
        // This will cause the helper's stdout to close, which makes the frame reader
        // loop end, which closes the frames channel, which makes the forward task end,
        // which closes the ASR streams.
        if let Some(helper) = state.helper.take() {
            helper.shutdown().await?;
        }

        // Abort spawned tasks to be safe + immediate (the natural shutdown above
        // may take a moment, and we want stop() to return promptly).
        if let Some(h) = state.forward_handle.take() {
            h.abort();
        }
        if let Some(h) = state.transcript_handle.take() {
            h.abort();
        }

        Ok(())
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
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
