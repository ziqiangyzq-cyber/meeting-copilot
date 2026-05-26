pub mod aliyun_paraformer;

use async_trait::async_trait;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    System,
    Mic,
}

#[async_trait]
pub trait ASRClient: Send + Sync {
    async fn push_pcm(&mut self, src: AudioSource, pcm: &[u8]) -> crate::error::Result<()>;
    async fn close(&mut self) -> crate::error::Result<()>;
}
