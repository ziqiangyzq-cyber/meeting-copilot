use crate::error::{AppError, Result};
use bytes::{Buf, BytesMut};
use tokio::io::AsyncReadExt;

pub const FRAME_MAGIC: u32 = 0xAB12CD34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    System,
    Mic,
}

impl AudioSource {
    fn from_tag(tag: u32) -> Result<Self> {
        match tag {
            0 => Ok(Self::System),
            1 => Ok(Self::Mic),
            _ => Err(AppError::AudioHelper(format!("unknown source tag: {tag}"))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub source: AudioSource,
    pub pcm: Vec<u8>,  // int16 LE 16kHz mono
}

pub struct FrameReader<R: AsyncReadExt + Unpin> {
    reader: R,
    buf: BytesMut,
}

impl<R: AsyncReadExt + Unpin> FrameReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: BytesMut::with_capacity(64 * 1024),
        }
    }

    /// Read the next frame. Returns Ok(None) on EOF.
    pub async fn next_frame(&mut self) -> Result<Option<AudioFrame>> {
        loop {
            // Need at least 12 bytes for header
            if self.buf.len() >= 12 {
                let magic = u32::from_le_bytes(self.buf[0..4].try_into().unwrap());
                if magic != FRAME_MAGIC {
                    // resync: drop one byte and try again
                    self.buf.advance(1);
                    continue;
                }
                let src_tag = u32::from_le_bytes(self.buf[4..8].try_into().unwrap());
                let size = u32::from_le_bytes(self.buf[8..12].try_into().unwrap()) as usize;

                if self.buf.len() >= 12 + size {
                    self.buf.advance(12);
                    let pcm = self.buf.split_to(size).to_vec();
                    let source = AudioSource::from_tag(src_tag)?;
                    return Ok(Some(AudioFrame { source, pcm }));
                }
            }

            // Need more data
            let mut chunk = [0u8; 8192];
            let n = self.reader.read(&mut chunk).await?;
            if n == 0 {
                // EOF
                return Ok(None);
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn build_frame(source: u32, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&FRAME_MAGIC.to_le_bytes());
        buf.extend_from_slice(&source.to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[tokio::test]
    async fn reads_single_frame() {
        let bytes = build_frame(0, &[0x01, 0x02, 0x03, 0x04]);
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::new(cursor));
        let frame = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(frame.source, AudioSource::System);
        assert_eq!(frame.pcm, vec![0x01, 0x02, 0x03, 0x04]);
    }

    #[tokio::test]
    async fn reads_multiple_frames() {
        let mut bytes = build_frame(0, &[1, 2]);
        bytes.extend(build_frame(1, &[3, 4, 5]));
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::new(cursor));
        let f1 = reader.next_frame().await.unwrap().unwrap();
        let f2 = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(f1.source, AudioSource::System);
        assert_eq!(f2.source, AudioSource::Mic);
        assert!(reader.next_frame().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn skips_garbage_before_magic() {
        let mut bytes = vec![0xFF, 0xEE, 0xDD];  // noise
        bytes.extend(build_frame(0, &[1, 2]));
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::new(cursor));
        let f = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(f.source, AudioSource::System);
        assert_eq!(f.pcm, vec![1, 2]);
    }

    #[tokio::test]
    async fn fragmented_read_assembles_frame() {
        // simulate stream that delivers half a frame, then the rest
        // (tests buffer accumulation across reads)
        let bytes = build_frame(1, &[10, 20, 30, 40, 50]);
        let cursor = Cursor::new(bytes);
        let mut reader = FrameReader::new(tokio::io::BufReader::with_capacity(3, cursor));
        let f = reader.next_frame().await.unwrap().unwrap();
        assert_eq!(f.source, AudioSource::Mic);
        assert_eq!(f.pcm, vec![10, 20, 30, 40, 50]);
    }
}
