// PCM frame types + parser. Implementation in Task 6.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSource {
    System,
    Mic,
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub source: AudioSource,
    pub pcm: Vec<u8>,
}
