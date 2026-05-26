use crate::db::Db;
use crate::error::{AppError, Result};
use crate::rag::{chunker, embedding::EmbeddingClient, parser};
use rusqlite::params;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const CHUNK_TARGET: usize = 500;
const CHUNK_OVERLAP: usize = 50;

/// Ingest a file: parse → chunk → embed → write to DB.
/// Returns the new material_id.
pub async fn ingest_file(
    db: &Db,
    embed: &EmbeddingClient,
    meeting_id: &str,
    file_path: &Path,
) -> Result<String> {
    // 1. Parse
    let text = parser::parse(file_path)?;

    // 2. Chunk
    let chunks = chunker::chunk(&text, CHUNK_TARGET, CHUNK_OVERLAP);
    if chunks.is_empty() {
        return Err(AppError::Config(format!(
            "no chunks produced from {}",
            file_path.display()
        )));
    }

    // 3. Embed (handles batching internally)
    let embeddings = embed.embed_batch(&chunks).await?;

    if embeddings.len() != chunks.len() {
        return Err(AppError::Asr(format!(
            "embedding count {} != chunks count {}",
            embeddings.len(),
            chunks.len()
        )));
    }

    // 4. Write to DB
    let material_id = Uuid::new_v4().simple().to_string();
    let file_name = file_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let conn = db.conn();

    // Use a transaction for atomicity
    conn.execute("BEGIN TRANSACTION", [])?;

    // Materials row
    let insert_material = conn.execute(
        "INSERT INTO materials (id, meeting_id, file_name, file_path, file_size, indexed_at, chunk_count) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![
            material_id,
            meeting_id,
            file_name,
            file_path.to_string_lossy(),
            text.len() as i64,
            now,
            chunks.len() as i64,
        ],
    );

    if let Err(e) = insert_material {
        let _ = conn.execute("ROLLBACK", []);
        return Err(e.into());
    }

    // Chunks + chunks_vec
    for (i, (chunk_text, vec)) in chunks.iter().zip(embeddings.iter()).enumerate() {
        let chunk_id: i64 = match conn.query_row(
            "INSERT INTO chunks (meeting_id, material_id, chunk_index, text) VALUES (?, ?, ?, ?) RETURNING id",
            params![meeting_id, material_id, i as i64, chunk_text],
            |r| r.get(0),
        ) {
            Ok(id) => id,
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                return Err(e.into());
            }
        };

        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        if let Err(e) = conn.execute(
            "INSERT INTO chunks_vec (rowid, embedding) VALUES (?, ?)",
            params![chunk_id, bytes],
        ) {
            let _ = conn.execute("ROLLBACK", []);
            return Err(e.into());
        }
    }

    conn.execute("COMMIT", [])?;
    Ok(material_id)
}
