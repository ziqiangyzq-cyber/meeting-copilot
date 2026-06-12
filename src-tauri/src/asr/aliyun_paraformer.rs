use crate::asr::{ASRClient, AudioSource};
use crate::error::{AppError, Result};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

const WS_URL: &str = "wss://dashscope.aliyuncs.com/api-ws/v1/inference/";
const MODEL: &str = "paraformer-realtime-v2";

/// Reconnect for up to ~5 minutes of continuous outage (backoff caps at 10s).
/// The attempt counter resets whenever a session reaches task-started, so a
/// long meeting can survive any number of separate network blips.
const MAX_RECONNECT_ATTEMPTS: u32 = 30;
/// After sending finish-task, how long to wait for the server's final results.
const FINISH_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct TranscriptEvent {
    pub source: AudioSource,
    pub text: String,
    pub is_final: bool,
    pub begin_ms: u64,
    pub end_ms: u64,
}

/// Connection-health events surfaced to the UI so a mid-meeting WS drop is
/// never silent.
#[derive(Debug, Clone)]
pub enum AsrStatus {
    Reconnecting { source: AudioSource, attempt: u32 },
    Reconnected { source: AudioSource },
    Failed { source: AudioSource },
}

#[derive(Debug, Serialize)]
struct RunTaskMsg {
    header: ClientHeader,
    payload: RunTaskPayload,
}

#[derive(Debug, Serialize)]
struct FinishTaskMsg {
    header: ClientHeader,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct ClientHeader {
    action: String,
    task_id: String,
    streaming: String,
}

#[derive(Debug, Serialize)]
struct RunTaskPayload {
    task_group: String,
    task: String,
    function: String,
    model: String,
    parameters: TaskParameters,
    input: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct TaskParameters {
    format: String,
    sample_rate: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    vocabulary_id: Option<String>,
    disfluency_removal_enabled: bool,
    language_hints: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ServerMsg {
    header: ServerHeader,
    payload: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ServerHeader {
    event: String,
    #[allow(dead_code)]
    #[serde(default)]
    task_id: String,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

pub struct AliyunParaformer {
    system_tx: Option<mpsc::Sender<Vec<u8>>>,
    mic_tx: Option<mpsc::Sender<Vec<u8>>>,
}

impl AliyunParaformer {
    pub async fn connect(
        api_key: String,
        vocabulary_id: Option<String>,
        transcript_tx: mpsc::Sender<TranscriptEvent>,
        status_tx: mpsc::Sender<AsrStatus>,
    ) -> Result<Self> {
        let (system_tx, system_rx) = mpsc::channel::<Vec<u8>>(256);
        let (mic_tx, mic_rx) = mpsc::channel::<Vec<u8>>(256);

        // First connection happens inline so a bad key / no network fails the
        // meeting start immediately; reconnects are handled by the supervisor.
        let system_ws = open_stream(&api_key).await?;
        let mic_ws = open_stream(&api_key).await?;

        tokio::spawn(supervise_stream(
            api_key.clone(),
            vocabulary_id.clone(),
            AudioSource::System,
            system_ws,
            system_rx,
            transcript_tx.clone(),
            status_tx.clone(),
        ));
        tokio::spawn(supervise_stream(
            api_key,
            vocabulary_id,
            AudioSource::Mic,
            mic_ws,
            mic_rx,
            transcript_tx,
            status_tx,
        ));

        Ok(Self {
            system_tx: Some(system_tx),
            mic_tx: Some(mic_tx),
        })
    }
}

#[async_trait]
impl ASRClient for AliyunParaformer {
    async fn push_pcm(&mut self, src: AudioSource, pcm: &[u8]) -> Result<()> {
        let tx = match src {
            AudioSource::System => self.system_tx.as_ref(),
            AudioSource::Mic => self.mic_tx.as_ref(),
        };
        let tx = tx.ok_or_else(|| AppError::Asr("stream closed".into()))?;
        tx.send(pcm.to_vec())
            .await
            .map_err(|_| AppError::Asr("ASR send channel closed".into()))?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        // Dropping senders triggers finish-task in the spawned tasks
        self.system_tx = None;
        self.mic_tx = None;
        Ok(())
    }
}

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// Open a raw WebSocket connection to DashScope (no task started yet).
async fn open_stream(api_key: &str) -> Result<WsStream> {
    let mut req = WS_URL
        .into_client_request()
        .map_err(|e| AppError::Asr(format!("invalid url: {e}")))?;
    req.headers_mut().insert(
        "Authorization",
        format!("Bearer {api_key}")
            .parse()
            .map_err(|e| AppError::Asr(format!("invalid auth header: {e}")))?,
    );
    req.headers_mut()
        .insert("X-DashScope-DataInspection", "enable".parse().unwrap());
    let (ws_stream, _resp) = connect_async(req).await?;
    Ok(ws_stream)
}

enum StreamEnd {
    /// Input channel closed and the session drained cleanly — meeting over.
    Finished,
    /// Connection/task died while audio was still flowing — reconnect.
    Disconnected { reason: String, saw_started: bool },
}

/// Owns one source's PCM receiver for the whole meeting; reconnects the
/// underlying WS session whenever it dies while audio is still flowing.
/// PCM keeps buffering in the channel during the gap, so short outages lose
/// little or no speech.
async fn supervise_stream(
    api_key: String,
    vocabulary_id: Option<String>,
    source: AudioSource,
    initial_ws: WsStream,
    mut pcm_rx: mpsc::Receiver<Vec<u8>>,
    transcript_tx: mpsc::Sender<TranscriptEvent>,
    status_tx: mpsc::Sender<AsrStatus>,
) {
    let mut ws = Some(initial_ws);
    let mut attempt: u32 = 0;
    loop {
        let stream = match ws.take() {
            Some(s) => s,
            None => {
                attempt += 1;
                if attempt > MAX_RECONNECT_ATTEMPTS {
                    tracing::error!(
                        "ASR {:?}: giving up after {} reconnect attempts",
                        source,
                        MAX_RECONNECT_ATTEMPTS
                    );
                    let _ = status_tx.send(AsrStatus::Failed { source }).await;
                    return;
                }
                let delay =
                    std::time::Duration::from_secs(2u64.pow((attempt - 1).min(4)).min(10));
                tracing::warn!(
                    "ASR {:?}: reconnecting (attempt {}) in {:?}",
                    source,
                    attempt,
                    delay
                );
                let _ = status_tx
                    .send(AsrStatus::Reconnecting { source, attempt })
                    .await;
                tokio::time::sleep(delay).await;
                match open_stream(&api_key).await {
                    Ok(s) => {
                        let _ = status_tx.send(AsrStatus::Reconnected { source }).await;
                        s
                    }
                    Err(e) => {
                        tracing::warn!("ASR {:?}: reconnect failed: {e}", source);
                        continue;
                    }
                }
            }
        };

        match run_stream_once(stream, &vocabulary_id, source, &mut pcm_rx, &transcript_tx).await
        {
            StreamEnd::Finished => {
                tracing::info!("ASR {:?}: session finished cleanly", source);
                return;
            }
            StreamEnd::Disconnected {
                reason,
                saw_started,
            } => {
                tracing::warn!("ASR {:?}: session dropped ({reason}); will reconnect", source);
                if saw_started {
                    // Session was healthy before dying — fresh backoff for the
                    // next outage instead of compounding old failures.
                    attempt = 0;
                }
            }
        }
    }
}

/// Run one ASR task over an already-open WS connection until the input closes
/// (clean finish) or the connection/task dies (caller reconnects).
async fn run_stream_once(
    ws: WsStream,
    vocabulary_id: &Option<String>,
    source: AudioSource,
    pcm_rx: &mut mpsc::Receiver<Vec<u8>>,
    transcript_tx: &mpsc::Sender<TranscriptEvent>,
) -> StreamEnd {
    let (mut write, mut read) = ws.split();
    let task_id = Uuid::new_v4().simple().to_string();

    let run_task = RunTaskMsg {
        header: ClientHeader {
            action: "run-task".into(),
            task_id: task_id.clone(),
            streaming: "duplex".into(),
        },
        payload: RunTaskPayload {
            task_group: "audio".into(),
            task: "asr".into(),
            function: "recognition".into(),
            model: MODEL.into(),
            parameters: TaskParameters {
                format: "pcm".into(),
                sample_rate: 16000,
                vocabulary_id: vocabulary_id.clone(),
                disfluency_removal_enabled: false,
                language_hints: vec!["zh".into(), "en".into()],
            },
            input: serde_json::json!({}),
        },
    };
    let run_json = match serde_json::to_string(&run_task) {
        Ok(j) => j,
        Err(e) => {
            return StreamEnd::Disconnected {
                reason: format!("serialize run-task: {e}"),
                saw_started: false,
            }
        }
    };
    if let Err(e) = write.send(Message::Text(run_json)).await {
        return StreamEnd::Disconnected {
            reason: format!("send run-task: {e}"),
            saw_started: false,
        };
    }

    let mut saw_started = false;
    loop {
        tokio::select! {
            pcm = pcm_rx.recv() => match pcm {
                Some(data) => {
                    if let Err(e) = write.send(Message::Binary(data)).await {
                        return StreamEnd::Disconnected {
                            reason: format!("send pcm: {e}"),
                            saw_started,
                        };
                    }
                }
                None => {
                    // Meeting over: flush finish-task, drain final results.
                    let finish = FinishTaskMsg {
                        header: ClientHeader {
                            action: "finish-task".into(),
                            task_id: task_id.clone(),
                            streaming: "duplex".into(),
                        },
                        payload: serde_json::json!({"input": {}}),
                    };
                    if let Ok(json) = serde_json::to_string(&finish) {
                        let _ = write.send(Message::Text(json)).await;
                    }
                    loop {
                        match tokio::time::timeout(FINISH_DRAIN_TIMEOUT, read.next()).await {
                            Ok(Some(Ok(Message::Text(text)))) => {
                                match handle_server_text(&text, source, transcript_tx).await {
                                    ServerOutcome::Continue | ServerOutcome::Started => {}
                                    ServerOutcome::Finished | ServerOutcome::Failed(_) => break,
                                }
                            }
                            Ok(Some(Ok(_))) => {}
                            _ => break, // Close / error / EOF / timeout — done either way
                        }
                    }
                    return StreamEnd::Finished;
                }
            },
            msg = read.next() => match msg {
                Some(Ok(Message::Text(text))) => {
                    match handle_server_text(&text, source, transcript_tx).await {
                        ServerOutcome::Continue => {}
                        ServerOutcome::Started => saw_started = true,
                        ServerOutcome::Failed(reason) => {
                            return StreamEnd::Disconnected { reason, saw_started }
                        }
                        ServerOutcome::Finished => {
                            return StreamEnd::Disconnected {
                                reason: "server finished task early".into(),
                                saw_started,
                            }
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    return StreamEnd::Disconnected {
                        reason: "server closed connection".into(),
                        saw_started,
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(e)) => {
                    return StreamEnd::Disconnected {
                        reason: format!("ws read: {e}"),
                        saw_started,
                    }
                }
                None => {
                    return StreamEnd::Disconnected {
                        reason: "ws stream ended".into(),
                        saw_started,
                    }
                }
            },
        }
    }
}

enum ServerOutcome {
    Continue,
    Started,
    Finished,
    Failed(String),
}

async fn handle_server_text(
    text: &str,
    source: AudioSource,
    transcript_tx: &mpsc::Sender<TranscriptEvent>,
) -> ServerOutcome {
    let server: ServerMsg = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("parse server msg failed: {e}, raw: {text}");
            return ServerOutcome::Continue;
        }
    };
    match server.header.event.as_str() {
        "task-started" => {
            tracing::info!("ASR task-started (source={:?})", source);
            ServerOutcome::Started
        }
        "result-generated" => {
            if let Some(payload) = server.payload {
                if let Some(output) = payload.get("output") {
                    parse_and_emit_transcript(output, source, transcript_tx).await;
                }
            }
            ServerOutcome::Continue
        }
        "task-failed" => {
            let reason = format!(
                "task-failed: code={:?} msg={:?}",
                server.header.error_code, server.header.error_message
            );
            tracing::error!("ASR {reason} (source={:?})", source);
            ServerOutcome::Failed(reason)
        }
        "task-finished" => {
            tracing::info!("ASR task-finished (source={:?})", source);
            ServerOutcome::Finished
        }
        other => {
            tracing::debug!("unhandled ASR event: {}", other);
            ServerOutcome::Continue
        }
    }
}

async fn parse_and_emit_transcript(
    output: &serde_json::Value,
    source: AudioSource,
    tx: &mpsc::Sender<TranscriptEvent>,
) {
    let Some(sentence) = output.get("sentence") else {
        return;
    };

    let text = sentence.get("text").and_then(|t| t.as_str()).unwrap_or("");
    let begin = sentence
        .get("begin_time")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let end = sentence
        .get("end_time")
        .and_then(|t| t.as_u64())
        .unwrap_or(0);
    let is_final = sentence
        .get("sentence_end")
        .and_then(|b| b.as_bool())
        .unwrap_or(false);

    if !text.is_empty() {
        let _ = tx
            .send(TranscriptEvent {
                source,
                text: text.into(),
                is_final,
                begin_ms: begin,
                end_ms: end,
            })
            .await;
    }
}
