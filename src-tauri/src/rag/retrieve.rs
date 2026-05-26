use crate::db::Db;
use crate::error::Result;
use crate::rag::embedding::EmbeddingClient;
use rusqlite::params;

#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk_id: i64,
    pub material_id: String,
    pub file_name: String,
    pub text: String,
    pub distance: f32,
}

/// Retrieve top-K chunks for a query within a meeting.
/// Distance is L2 by default in sqlite-vec; smaller = more similar.
pub async fn retrieve(
    db: &Db,
    embed: &EmbeddingClient,
    meeting_id: &str,
    query: &str,
    k: usize,
) -> Result<Vec<RetrievedChunk>> {
    let q_vec = embed
        .embed_batch(&[query.to_string()])
        .await?
        .into_iter()
        .next()
        .expect("embed_batch returned empty for non-empty query");
    let q_bytes: Vec<u8> = q_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

    let conn = db.conn();
    // sqlite-vec KNN: `WHERE embedding MATCH ? AND k = ?` is the documented form for vec0
    let mut stmt = conn.prepare(
        "SELECT c.id, c.material_id, m.file_name, c.text, v.distance
         FROM chunks_vec v
         JOIN chunks c ON c.id = v.rowid
         JOIN materials m ON m.id = c.material_id
         WHERE v.embedding MATCH ?
           AND k = ?
           AND c.meeting_id = ?
         ORDER BY v.distance",
    )?;

    let rows = stmt.query_map(params![q_bytes, k as i64, meeting_id], |r| {
        Ok(RetrievedChunk {
            chunk_id: r.get(0)?,
            material_id: r.get(1)?,
            file_name: r.get(2)?,
            text: r.get(3)?,
            distance: r.get::<_, f64>(4)? as f32,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}
