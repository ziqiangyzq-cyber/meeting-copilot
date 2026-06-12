use crate::asr::aliyun_paraformer::{AliyunParaformer, AsrStatus, TranscriptEvent};
use crate::asr::{ASRClient, AudioSource as AsrSource};
use crate::audio_pump::{frame::AudioSource as PumpSource, HelperProc};
use crate::config::{Config, LlmProvider};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::llm::{minimax::MiniMaxClient, openai_compat::OpenAICompatClient, LLMClient};
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

/// A3: if NO audio frame arrives from any source within this window, the whole
/// capture pipeline is considered dead (helper died / both streams stopped).
const FRAME_STALL_TIMEOUT: Duration = Duration::from_secs(8);

/// A3: if one source (e.g. system audio) was active but goes silent this long
/// while the other source keeps delivering, that capture path has stalled. Set
/// a bit higher than the global timeout to tolerate natural conversational gaps
/// where one side simply isn't speaking.
const SOURCE_STALL_TIMEOUT: Duration = Duration::from_secs(12);

/// Stall recovery: how many restart attempts per source before declaring the
/// capture path dead (terminal audio_stalled), and how long to wait for frames
/// to resume after each restart command.
const SOURCE_RECOVERY_MAX_ATTEMPTS: u32 = 3;
const SOURCE_RECOVERY_DEADLINE: Duration = Duration::from_secs(15);

/// A source that has NEVER delivered a frame by this long after meeting start
/// is dead-on-arrival (e.g. meeting began mid-Bluetooth-handshake). The regular
/// per-source stall detector only watches previously-active sources, so this
/// grace check is what catches the from-the-start failures.
const SOURCE_STARTUP_GRACE: Duration = Duration::from_secs(10);

/// B1: RMS (over int16 samples) below this is treated as silence and not sent to
/// ASR. Frames are 16 kHz / 16-bit LE / mono. Speech is typically in the
/// thousands; a quiet (noise-suppressed) room sits near zero. Conservative so we
/// never clip real speech onsets.
const SILENCE_RMS_THRESHOLD: f32 = 200.0;

pub struct Orchestrator {
    inner: Arc<Mutex<OrchestratorState>>,
    db: Arc<Db>,
    embed: Arc<RwLock<Arc<EmbeddingClient>>>,
    llm: Arc<RwLock<Arc<dyn LLMClient>>>,
    config: Arc<RwLock<Config>>,
    /// User-facing mic toggle. System audio is always captured; this only gates
    /// the mic path. Read by the frame forwarder (stall detection + frame drop)
    /// and reset to ON at each meeting start.
    mic_enabled: Arc<std::sync::atomic::AtomicBool>,
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
        let llm = build_llm(config);
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
            mic_enabled: Arc::new(std::sync::atomic::AtomicBool::new(true)),
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

    pub fn current_minimax_key(&self) -> String {
        self.config.read().unwrap().minimax_api_key.clone()
    }

    /// Clone the in-memory Config so commands can mutate + reconfigure cleanly.
    pub fn current_config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    /// Returns true if Aliyun key + provider-specific LLM credentials are all set.
    pub fn has_keys(&self) -> bool {
        let cfg = self.config.read().unwrap();
        if cfg.aliyun_api_key.trim().is_empty() {
            return false;
        }
        match cfg.llm_provider {
            LlmProvider::MiniMax => !cfg.minimax_api_key.trim().is_empty(),
            LlmProvider::OpenAICompat => {
                !cfg.llm_base_url.trim().is_empty()
                    && !cfg.llm_model.trim().is_empty()
                    && !cfg.llm_api_key.trim().is_empty()
            }
        }
    }

    pub fn reconfigure(&self, config: &Config) {
        let new_embed = Arc::new(EmbeddingClient::new(config.aliyun_api_key.clone()));
        let new_llm = build_llm(config);
        *self.embed.write().unwrap() = new_embed;
        *self.llm.write().unwrap() = new_llm;
        *self.config.write().unwrap() = config.clone();
        tracing::info!("orchestrator clients reconfigured (provider={:?})", config.llm_provider);
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
        let bin_path = locate_helper_binary(&app)?;
        let mut helper = HelperProc::spawn(bin_path).await?;
        let (voice_processing, lock_builtin_mic) = {
            let cfg = self.config.read().unwrap();
            (cfg.voice_processing_enabled, cfg.lock_builtin_mic)
        };
        helper.send_start(voice_processing, lock_builtin_mic).await?;

        // Mic always starts ON for each meeting (deliberate: a remembered OFF
        // state would silently drop the user's own speech for the whole meeting).
        self.mic_enabled
            .store(true, std::sync::atomic::Ordering::Relaxed);

        // 2. Connect ASR. Connection-health events (reconnecting / failed) are
        // forwarded to the UI as "asr_status" so a mid-meeting WS drop is visible.
        let (transcript_tx, mut transcript_rx) = mpsc::channel::<TranscriptEvent>(64);
        let (asr_status_tx, mut asr_status_rx) = mpsc::channel::<AsrStatus>(16);
        let asr = AliyunParaformer::connect(
            self.current_aliyun_key(),
            None,
            transcript_tx,
            asr_status_tx,
        )
        .await?;
        let asr = Arc::new(Mutex::new(asr));

        let app_for_asr_status = app.clone();
        tokio::spawn(async move {
            while let Some(status) = asr_status_rx.recv().await {
                let payload = match status {
                    AsrStatus::Reconnecting { source, attempt } => serde_json::json!({
                        "source": src_name(source), "state": "reconnecting", "attempt": attempt,
                    }),
                    AsrStatus::Reconnected { source } => serde_json::json!({
                        "source": src_name(source), "state": "reconnected",
                    }),
                    AsrStatus::Failed { source } => serde_json::json!({
                        "source": src_name(source), "state": "failed",
                    }),
                };
                let _ = app_for_asr_status.emit("asr_status", payload);
            }
        });

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
        let app_for_pump = app.clone();
        let mic_enabled_for_pump = self.mic_enabled.clone();
        let commander_for_pump = helper.commander();
        let forward = tokio::spawn(async move {
            let mut rx = frames_rx;
            // A3: per-source stall detection. The real failure today is one capture
            // path dying (e.g. system audio / SCStream stops) while the other (mic)
            // keeps delivering. A pure "no frames at all" check misses that, so we
            // track the last frame time per source and fire if a *previously active*
            // source goes quiet past the timeout. Index 0 = system, 1 = mic.
            let mut last_seen: [Option<std::time::Instant>; 2] = [None, None];
            // Stall recovery state per source: restart attempts used, and the
            // deadline by which frames must resume after a restart command.
            // A transient capture death (display sleep, device hiccup) now gets
            // up to SOURCE_RECOVERY_MAX_ATTEMPTS automatic restarts before the
            // terminal audio_stalled is declared.
            let mut recover_attempts: [u32; 2] = [0, 0];
            let mut recover_deadline: [Option<std::time::Instant>; 2] = [None, None];
            // Per-source grace anchor: meeting start, or for the mic, the moment
            // it was last re-enabled (so the DOA check doesn't fire the instant
            // the user toggles the mic back on).
            let mut grace_start: [std::time::Instant; 2] = [std::time::Instant::now(); 2];
            let mut mic_on_prev = true;
            loop {
                match tokio::time::timeout(FRAME_STALL_TIMEOUT, rx.recv()).await {
                    Ok(Some(frame)) => {
                        // Mic toggle: while OFF, drop any straggler mic frames and
                        // erase mic's last_seen so stall detection (a) doesn't fire
                        // "mic went silent" while it's deliberately off, and (b)
                        // doesn't see a stale pre-toggle timestamp right after re-enable.
                        let mic_on =
                            mic_enabled_for_pump.load(std::sync::atomic::Ordering::Relaxed);
                        if mic_on && !mic_on_prev {
                            grace_start[1] = std::time::Instant::now();
                        }
                        mic_on_prev = mic_on;
                        if !mic_on {
                            last_seen[1] = None;
                            recover_deadline[1] = None;
                            recover_attempts[1] = 0;
                            if frame.source == PumpSource::Mic {
                                continue;
                            }
                        }

                        let now = std::time::Instant::now();
                        let idx = match frame.source {
                            PumpSource::System => 0,
                            PumpSource::Mic => 1,
                        };
                        last_seen[idx] = Some(now);

                        // Frames resumed on a source we were recovering → recovered.
                        if recover_deadline[idx].take().is_some() {
                            recover_attempts[idx] = 0;
                            tracing::info!("audio recovery: {} frames resumed", src_label(idx));
                            let _ = app_for_pump.emit(
                                "audio_recovered",
                                serde_json::json!({ "source": src_label(idx) }),
                            );
                        }

                        let other = 1 - idx;
                        if let Some(deadline) = recover_deadline[other] {
                            // Recovery in progress for the other source: frames
                            // haven't resumed yet — retry or give up at deadline.
                            if now > deadline {
                                if recover_attempts[other] >= SOURCE_RECOVERY_MAX_ATTEMPTS {
                                    tracing::warn!(
                                        "audio stall: {} source unrecoverable after {} restart attempts",
                                        src_label(other),
                                        recover_attempts[other]
                                    );
                                    let _ = app_for_pump.emit("audio_stalled", ());
                                    break;
                                }
                                recover_attempts[other] += 1;
                                start_source_recovery(
                                    &commander_for_pump,
                                    &app_for_pump,
                                    other,
                                    recover_attempts[other],
                                )
                                .await;
                                recover_deadline[other] = Some(now + SOURCE_RECOVERY_DEADLINE);
                            }
                        } else if let Some(t) = last_seen[other] {
                            if now.duration_since(t) > SOURCE_STALL_TIMEOUT {
                                // Previously-active source went silent. Instead of the
                                // old terminal break, restart its capture path.
                                recover_attempts[other] += 1;
                                start_source_recovery(
                                    &commander_for_pump,
                                    &app_for_pump,
                                    other,
                                    recover_attempts[other],
                                )
                                .await;
                                last_seen[other] = None;
                                recover_deadline[other] = Some(now + SOURCE_RECOVERY_DEADLINE);
                            }
                        } else if recover_attempts[other] == 0
                            && (other == 0 || mic_on)
                            && now.duration_since(grace_start[other]) > SOURCE_STARTUP_GRACE
                        {
                            // Startup grace: the other source has NEVER delivered a
                            // frame — dead-on-arrival capture path. Kick recovery;
                            // subsequent retries run through the deadline branch.
                            recover_attempts[other] = 1;
                            start_source_recovery(
                                &commander_for_pump,
                                &app_for_pump,
                                other,
                                1,
                            )
                            .await;
                            recover_deadline[other] = Some(now + SOURCE_RECOVERY_DEADLINE);
                        }

                        // B1: silence gate. Don't feed near-silent frames to ASR —
                        // Paraformer-realtime hallucinates ad/finance-style text when
                        // fed continuous silence. last_seen is updated above first, so
                        // gating a quiet frame never counts as a stall.
                        if frame_rms(&frame.pcm) < SILENCE_RMS_THRESHOLD {
                            continue;
                        }
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
                    Ok(None) => break, // channel closed normally (meeting stopped)
                    Err(_) => {
                        // No frames from *any* source for the timeout → whole pipeline dead.
                        tracing::warn!(
                            "audio stall: no frames for {}s, capture pipeline appears dead",
                            FRAME_STALL_TIMEOUT.as_secs()
                        );
                        let _ = app_for_pump.emit("audio_stalled", ());
                        break;
                    }
                }
            }
            // Frame channel closed / stalled — close ASR streams so finish-task is sent
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

    /// Manually trigger a mic restart in AudioHelper (fallback for when
    /// AVAudioEngineConfigurationChange doesn't fire on device hot-swap).
    pub async fn restart_mic(&self) -> Result<()> {
        let mut state = self.inner.lock().await;
        if let Some(helper) = state.helper.as_mut() {
            helper.send_cmd("restart_mic").await?;
            tracing::info!("sent restart_mic to AudioHelper");
            Ok(())
        } else {
            Err(AppError::AudioHelper("no active meeting".into()))
        }
    }

    /// Live mic on/off toggle. System audio keeps flowing regardless. The flag is
    /// flipped first so the frame forwarder reacts even if the helper command lags.
    pub async fn set_mic_enabled(&self, enabled: bool) -> Result<()> {
        self.mic_enabled
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        let mut state = self.inner.lock().await;
        if let Some(helper) = state.helper.as_mut() {
            helper.send_set_mic_enabled(enabled).await?;
            tracing::info!("sent set_mic_enabled={} to AudioHelper", enabled);
        } else {
            tracing::info!("no active meeting; mic toggle ignored (mic is always ON at next start)");
        }
        Ok(())
    }

    /// Apply voice processing setting LIVE if a meeting is running.
    /// Returns Ok regardless — if no meeting, this is a no-op (config update is done separately).
    pub async fn apply_voice_processing_live(&self, enabled: bool) -> Result<()> {
        let mut state = self.inner.lock().await;
        if let Some(helper) = state.helper.as_mut() {
            helper.send_set_voice_processing(enabled).await?;
            tracing::info!("sent set_voice_processing={} to AudioHelper (live)", enabled);
        } else {
            tracing::info!("no active meeting; set_voice_processing will apply on next start");
        }
        Ok(())
    }

    /// Apply lock-builtin-mic setting LIVE if a meeting is running (AudioHelper
    /// restarts the mic on the chosen device). No-op when no meeting.
    pub async fn apply_lock_builtin_mic_live(&self, enabled: bool) -> Result<()> {
        let mut state = self.inner.lock().await;
        if let Some(helper) = state.helper.as_mut() {
            helper.send_set_lock_builtin_mic(enabled).await?;
            tracing::info!("sent set_lock_builtin_mic={} to AudioHelper (live)", enabled);
        } else {
            tracing::info!("no active meeting; lock_builtin_mic will apply on next start");
        }
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

fn src_label(idx: usize) -> &'static str {
    if idx == 0 {
        "system"
    } else {
        "mic"
    }
}

fn src_name(s: AsrSource) -> &'static str {
    match s {
        AsrSource::System => "system",
        AsrSource::Mic => "mic",
    }
}

/// Emit the recovering event + send the per-source restart command to AudioHelper.
async fn start_source_recovery(
    commander: &crate::audio_pump::HelperCommander,
    app: &tauri::AppHandle,
    idx: usize,
    attempt: u32,
) {
    let cmd = if idx == 0 { "restart_system" } else { "restart_mic" };
    tracing::warn!(
        "audio stall: {} source silent; sending {cmd} (attempt {attempt})",
        src_label(idx)
    );
    let _ = app.emit(
        "audio_recovering",
        serde_json::json!({ "source": src_label(idx), "attempt": attempt }),
    );
    if let Err(e) = commander.send_cmd(cmd).await {
        tracing::warn!("send {cmd} failed: {e}");
    }
}

/// RMS energy of a 16-bit LE PCM buffer. Used by the B1 silence gate.
fn frame_rms(pcm: &[u8]) -> f32 {
    let n = pcm.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let mut sum_sq = 0.0f64;
    for ch in pcm.chunks_exact(2) {
        let s = i16::from_le_bytes([ch[0], ch[1]]) as f64;
        sum_sq += s * s;
    }
    ((sum_sq / n as f64).sqrt()) as f32
}

fn build_llm(config: &Config) -> Arc<dyn LLMClient> {
    match config.llm_provider {
        LlmProvider::MiniMax => Arc::new(MiniMaxClient::new(config.minimax_api_key.clone())),
        LlmProvider::OpenAICompat => Arc::new(OpenAICompatClient::new(
            config.llm_base_url.clone(),
            config.llm_api_key.clone(),
            config.llm_model.clone(),
        )),
    }
}

fn locate_helper_binary(app: &tauri::AppHandle) -> Result<PathBuf> {
    use tauri::Manager;

    // Priority:
    // 1. AUDIO_HELPER_PATH env (override for dev / debugging)
    // 2. Production: Tauri app resource dir (.app/Contents/Resources/... or %INSTALLDIR%\resources\)
    // 3. Dev paths relative to where the Tauri binary runs

    let binary_name = if cfg!(target_os = "windows") {
        "AudioHelper.exe"
    } else {
        "AudioHelper"
    };

    if let Ok(p) = std::env::var("AUDIO_HELPER_PATH") {
        return Ok(PathBuf::from(p));
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("resources").join(binary_name);
        if bundled.exists() {
            return Ok(bundled);
        }
        // Some Tauri layouts flatten resources (no /resources/ prefix)
        let bundled_alt = resource_dir.join(binary_name);
        if bundled_alt.exists() {
            return Ok(bundled_alt);
        }
    }

    let dev_candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            "audio-helper-win/target/release/AudioHelper.exe",
            "../audio-helper-win/target/release/AudioHelper.exe",
        ]
    } else {
        &[
            "audio-helper/.build/release/AudioHelper",
            "../audio-helper/.build/release/AudioHelper",
        ]
    };
    for path in dev_candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    Err(AppError::AudioHelper(
        format!("AudioHelper binary not found ({binary_name}); build with `pnpm tauri build` or the platform-specific dev build (macOS: `cd audio-helper && swift build -c release`, Windows: `cd audio-helper-win && cargo build --release`)")
    ))
}
