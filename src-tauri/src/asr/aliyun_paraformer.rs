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

#[derive(Debug, Clone)]
pub struct TranscriptEvent {
    pub source: AudioSource,
    pub text: String,
    pub is_final: bool,
    pub begin_ms: u64,
    pub end_ms: u64,
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
    ) -> Result<Self> {
        let (system_tx, system_rx) = mpsc::channel::<Vec<u8>>(256);
        let (mic_tx, mic_rx) = mpsc::channel::<Vec<u8>>(256);

        spawn_stream(
            api_key.clone(),
            vocabulary_id.clone(),
            AudioSource::System,
            system_rx,
            transcript_tx.clone(),
        )
        .await?;
        spawn_stream(
            api_key,
            vocabulary_id,
            AudioSource::Mic,
            mic_rx,
            transcript_tx,
        )
        .await?;

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

async fn spawn_stream(
    api_key: String,
    vocabulary_id: Option<String>,
    source: AudioSource,
    mut pcm_rx: mpsc::Receiver<Vec<u8>>,
    transcript_tx: mpsc::Sender<TranscriptEvent>,
) -> Result<()> {
    let task_id = Uuid::new_v4().simple().to_string();

    // Build the WebSocket connect request with auth header
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
    let (mut write, mut read) = ws_stream.split();

    // Send run-task as first message
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
                vocabulary_id,
                disfluency_removal_enabled: false,
                language_hints: vec!["zh".into(), "en".into()],
            },
            input: serde_json::json!({}),
        },
    };
    let run_json = serde_json::to_string(&run_task)?;
    write.send(Message::Text(run_json)).await?;

    let task_id_clone = task_id.clone();

    // Audio send loop
    tokio::spawn(async move {
        while let Some(pcm) = pcm_rx.recv().await {
            if write.send(Message::Binary(pcm)).await.is_err() {
                break;
            }
        }
        // Send finish-task on receiver close
        let finish = FinishTaskMsg {
            header: ClientHeader {
                action: "finish-task".into(),
                task_id: task_id_clone,
                streaming: "duplex".into(),
            },
            payload: serde_json::json!({"input": {}}),
        };
        if let Ok(json) = serde_json::to_string(&finish) {
            let _ = write.send(Message::Text(json)).await;
        }
        // Don't close the socket immediately — let the server send task-finished first
    });

    // Transcript receive loop
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let server: ServerMsg = match serde_json::from_str(&text) {
                        Ok(m) => m,
                        Err(e) => {
                            tracing::warn!("parse server msg failed: {e}, raw: {text}");
                            continue;
                        }
                    };
                    match server.header.event.as_str() {
                        "task-started" => {
                            tracing::info!("ASR task-started (source={:?})", source);
                        }
                        "result-generated" => {
                            if let Some(payload) = server.payload {
                                if let Some(output) = payload.get("output") {
                                    parse_and_emit_transcript(output, source, &transcript_tx).await;
                                }
                            }
                        }
                        "task-failed" => {
                            tracing::error!(
                                "ASR task failed: code={:?}, msg={:?}",
                                server.header.error_code,
                                server.header.error_message
                            );
                            eprintln!(
                                "ASR task-failed raw: {text}"
                            );
                            break;
                        }
                        "task-finished" => {
                            tracing::info!("ASR task-finished (source={:?})", source);
                            break;
                        }
                        other => {
                            tracing::debug!("unhandled ASR event: {}", other);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("ASR ws closed (source={:?})", source);
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("ASR ws read error: {e}");
                    break;
                }
            }
        }
    });

    Ok(())
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
