pub mod models;
pub mod schema;

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Open a database at `path` (creating if missing), load the sqlite-vec
    /// extension, and run schema migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        // Register sqlite-vec as an auto-extension BEFORE opening the connection
        // so the vec0 virtual table is available during schema init.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        let conn = Connection::open(path.as_ref())?;
        schema::init(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn conn(&self) -> std::sync::MutexGuard<Connection> {
        self.conn.lock().expect("db mutex poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn fresh_db() -> (tempfile::TempDir, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("test.sqlite");
        let db = Db::open(&path).expect("open db");
        (tmp, db)
    }

    #[test]
    fn opens_db_and_runs_schema() {
        let (_tmp, db) = fresh_db();
        let conn = db.conn();
        // All 6 base tables exist
        for table in ["meetings", "materials", "chunks", "transcripts", "suggestions"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} not created");
        }
        // chunks_vec virtual table exists
        let vec_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='chunks_vec'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(vec_count, 1, "chunks_vec virtual table not created");
    }

    #[test]
    fn insert_and_select_meeting() {
        let (_tmp, db) = fresh_db();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO meetings (id, name, started_at) VALUES (?, ?, ?)",
            params!["m1", "test meeting", 1234567890_i64],
        )
        .unwrap();
        let (name, started_at): (String, i64) = conn
            .query_row(
                "SELECT name, started_at FROM meetings WHERE id = ?",
                ["m1"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "test meeting");
        assert_eq!(started_at, 1234567890);
    }

    #[test]
    fn chunks_vec_accepts_1024_dim_vector() {
        let (_tmp, db) = fresh_db();
        let conn = db.conn();

        // Insert a parent meeting + material + chunk
        conn.execute(
            "INSERT INTO meetings (id, name, started_at) VALUES (?, ?, ?)",
            params!["m1", "t", 0_i64],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO materials (id, meeting_id, file_name, file_path) VALUES (?, ?, ?, ?)",
            params!["mat1", "m1", "test.md", "/tmp/test.md"],
        )
        .unwrap();
        let chunk_id: i64 = conn
            .query_row(
                "INSERT INTO chunks (meeting_id, material_id, chunk_index, text) VALUES (?, ?, ?, ?) RETURNING id",
                params!["m1", "mat1", 0_i64, "hello chunk"],
                |r| r.get(0),
            )
            .unwrap();

        // Make a 1024-dim f32 vector, pack as LE bytes
        let vec: Vec<f32> = (0..1024).map(|i| (i as f32) / 1024.0).collect();
        let bytes: Vec<u8> = vec.iter().flat_map(|f| f.to_le_bytes()).collect();
        assert_eq!(bytes.len(), 4096);

        conn.execute(
            "INSERT INTO chunks_vec (rowid, embedding) VALUES (?, ?)",
            params![chunk_id, bytes],
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunks_vec", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
