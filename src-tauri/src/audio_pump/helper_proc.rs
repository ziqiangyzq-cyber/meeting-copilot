use crate::audio_pump::frame::{AudioFrame, FrameReader};
use crate::error::{AppError, Result};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct HelperProc {
    child: Child,
    stdin: ChildStdin,
    /// Receiver of parsed PCM frames. Use `take_frames()` to move out.
    frames_rx: Option<mpsc::Receiver<AudioFrame>>,
}

impl HelperProc {
    /// Spawn the AudioHelper binary at `binary_path`.
    pub async fn spawn(binary_path: PathBuf) -> Result<Self> {
        let mut child = Command::new(&binary_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AppError::AudioHelper(format!("spawn failed: {e}")))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| AppError::AudioHelper("no stdin".into()))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| AppError::AudioHelper("no stdout".into()))?;
        let stderr = child.stderr.take()
            .ok_or_else(|| AppError::AudioHelper("no stderr".into()))?;

        // Forward stderr (JSON log lines) into tracing
        tokio::spawn(forward_stderr(stderr));

        // Spawn the frame reader loop
        let (frames_tx, frames_rx) = mpsc::channel(256);
        tokio::spawn(read_frames_loop(stdout, frames_tx));

        Ok(Self {
            child,
            stdin,
            frames_rx: Some(frames_rx),
        })
    }

    /// Send a JSON command line to AudioHelper stdin.
    pub async fn send_cmd(&mut self, cmd: &str) -> Result<()> {
        let line = format!("{{\"cmd\":\"{cmd}\"}}\n");
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Send start command with options.
    pub async fn send_start(&mut self, voice_processing: bool) -> Result<()> {
        let line = format!(
            "{{\"cmd\":\"start\",\"voice_processing\":{}}}\n",
            voice_processing
        );
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Send live set_voice_processing command (AudioHelper will restart mic).
    pub async fn send_set_voice_processing(&mut self, enabled: bool) -> Result<()> {
        let line = format!(
            "{{\"cmd\":\"set_voice_processing\",\"voice_processing\":{}}}\n",
            enabled
        );
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Take ownership of the frame receiver. Returns None if already taken.
    pub fn take_frames(&mut self) -> Option<mpsc::Receiver<AudioFrame>> {
        self.frames_rx.take()
    }

    /// Send stop command and wait for child to exit.
    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.send_cmd("stop").await;
        let _ = self.child.wait().await;
        Ok(())
    }
}

async fn read_frames_loop(stdout: ChildStdout, tx: mpsc::Sender<AudioFrame>) {
    let mut reader = FrameReader::new(BufReader::new(stdout));
    loop {
        match reader.next_frame().await {
            Ok(Some(frame)) => {
                if tx.send(frame).await.is_err() {
                    info!("frames receiver closed, stopping frame reader");
                    return;
                }
            }
            Ok(None) => {
                info!("AudioHelper stdout closed (EOF)");
                return;
            }
            Err(e) => {
                error!("frame read error: {}", e);
                return;
            }
        }
    }
}

async fn forward_stderr(stderr: tokio::process::ChildStderr) {
    let mut reader = BufReader::new(stderr).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        info!("[AudioHelper] {}", line);
    }
}
