use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub name: String,
    pub project_ref: Option<String>,
    pub purpose: Option<String>,
    pub participants: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub audio_path: Option<String>,
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub meeting_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: Option<i64>,
    pub indexed_at: Option<i64>,
    pub chunk_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: i64,
    pub meeting_id: String,
    pub material_id: String,
    pub chunk_index: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRow {
    pub id: i64,
    pub meeting_id: String,
    pub speaker: Option<String>,
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub is_final: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionRow {
    pub id: i64,
    pub meeting_id: String,
    pub triggered_at: i64,
    pub trigger_type: Option<String>,
    pub style: Option<String>,
    pub content: String,
    pub user_action: Option<String>,
}
