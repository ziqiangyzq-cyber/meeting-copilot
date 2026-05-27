use crate::error::Result;
use rusqlite::Connection;

const DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS meetings (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        project_ref TEXT,
        purpose TEXT,
        participants TEXT,
        started_at INTEGER NOT NULL,
        ended_at INTEGER,
        audio_path TEXT,
        metadata TEXT
    )",
    "CREATE TABLE IF NOT EXISTS materials (
        id TEXT PRIMARY KEY,
        meeting_id TEXT NOT NULL,
        file_name TEXT NOT NULL,
        file_path TEXT NOT NULL,
        file_size INTEGER,
        indexed_at INTEGER,
        chunk_count INTEGER,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id)
    )",
    "CREATE TABLE IF NOT EXISTS chunks (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        material_id TEXT NOT NULL,
        chunk_index INTEGER NOT NULL,
        text TEXT NOT NULL,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id),
        FOREIGN KEY (material_id) REFERENCES materials(id)
    )",
    "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_vec USING vec0(
        embedding float[1024]
    )",
    "CREATE TABLE IF NOT EXISTS transcripts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        speaker TEXT,
        text TEXT NOT NULL,
        start_ms INTEGER NOT NULL,
        end_ms INTEGER NOT NULL,
        is_final INTEGER DEFAULT 0,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id)
    )",
    "CREATE TABLE IF NOT EXISTS suggestions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        triggered_at INTEGER NOT NULL,
        trigger_type TEXT,
        style TEXT,
        content TEXT NOT NULL,
        user_action TEXT,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id)
    )",
    "CREATE TABLE IF NOT EXISTS minutes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        meeting_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        markdown TEXT NOT NULL,
        generated_at INTEGER NOT NULL,
        model_used TEXT,
        tokens_used INTEGER,
        FOREIGN KEY (meeting_id) REFERENCES meetings(id)
    )",
    "CREATE INDEX IF NOT EXISTS idx_chunks_meeting ON chunks(meeting_id)",
    "CREATE INDEX IF NOT EXISTS idx_transcripts_meeting ON transcripts(meeting_id)",
    "CREATE INDEX IF NOT EXISTS idx_suggestions_meeting ON suggestions(meeting_id)",
    "CREATE INDEX IF NOT EXISTS idx_minutes_meeting ON minutes(meeting_id)",
];

pub fn init(conn: &Connection) -> Result<()> {
    for stmt in DDL {
        conn.execute(stmt, [])?;
    }

    // Idempotent migration: add focus_points column to meetings if missing.
    // SQLite errors on duplicate column add; we swallow that specific error.
    let _ = conn.execute("ALTER TABLE meetings ADD COLUMN focus_points TEXT", []);

    Ok(())
}
