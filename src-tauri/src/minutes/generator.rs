use crate::db::models::{Meeting, SuggestionRow, TranscriptRow};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::llm::{LLMClient, Message};
use crate::minutes::prompt::{system_prompt, user_prompt, MinutesContext};
use rusqlite::params;
use std::sync::Arc;
use tokio::sync::mpsc;

pub struct MinutesGenerator {
    db: Arc<Db>,
    llm: Arc<dyn LLMClient>,
}

impl MinutesGenerator {
    pub fn new(db: Arc<Db>, llm: Arc<dyn LLMClient>) -> Self {
        Self { db, llm }
    }

    /// Generate minutes for a meeting. Streams tokens via `out`. Writes final
    /// markdown to the `minutes` table. Returns the complete markdown.
    pub async fn generate(
        &self,
        meeting_id: &str,
        out: mpsc::Sender<String>,
    ) -> Result<String> {
        // Load meeting + transcripts + suggestions
        let (meeting, transcripts, suggestions) = self.load_context(meeting_id)?;

        let template = crate::templates::get_by_id(
            meeting.template_id.as_deref().unwrap_or("default"),
        );

        let ctx = MinutesContext {
            meeting: &meeting,
            transcripts: &transcripts,
            suggestions: &suggestions,
        };

        let system = system_prompt();
        let user = user_prompt(&ctx, &template);

        // Stream via LLM, accumulating to return at end
        let (tx, mut rx) = mpsc::channel::<String>(256);
        let llm = self.llm.clone();
        let messages = vec![Message::system(system), Message::user(user)];
        let llm_task = tokio::spawn(async move { llm.stream(messages, tx).await });

        // Forward each token to both the public out channel + accumulate
        let mut markdown = String::new();
        while let Some(tok) = rx.recv().await {
            markdown.push_str(&tok);
            if out.send(tok).await.is_err() {
                // receiver dropped — keep collecting LLM tokens but stop forwarding
                while let Some(more) = rx.recv().await {
                    markdown.push_str(&more);
                }
                break;
            }
        }

        llm_task
            .await
            .map_err(|e| AppError::Asr(format!("minutes llm join: {e}")))?
            .map_err(|e| AppError::Asr(format!("minutes llm failed: {e}")))?;

        // Write to minutes table (versioned)
        self.persist(meeting_id, &markdown)?;

        Ok(markdown)
    }

    fn load_context(
        &self,
        meeting_id: &str,
    ) -> Result<(Meeting, Vec<TranscriptRow>, Vec<SuggestionRow>)> {
        let conn = self.db.conn();

        let meeting: Meeting = conn.query_row(
            "SELECT id, name, project_ref, purpose, participants, started_at, ended_at, audio_path, metadata, focus_points, notes, template_id FROM meetings WHERE id = ?",
            [meeting_id],
            |r| Ok(Meeting {
                id: r.get(0)?,
                name: r.get(1)?,
                project_ref: r.get(2)?,
                purpose: r.get(3)?,
                participants: r.get(4)?,
                started_at: r.get(5)?,
                ended_at: r.get(6)?,
                audio_path: r.get(7)?,
                metadata: r.get(8)?,
                focus_points: r.get(9)?,
                notes: r.get(10)?,
                template_id: r.get(11)?,
            }),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, speaker, text, start_ms, end_ms, is_final FROM transcripts WHERE meeting_id = ? AND is_final = 1 ORDER BY start_ms"
        )?;
        let transcripts: Vec<TranscriptRow> = stmt
            .query_map([meeting_id], |r| {
                Ok(TranscriptRow {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    speaker: r.get(2)?,
                    text: r.get(3)?,
                    start_ms: r.get(4)?,
                    end_ms: r.get(5)?,
                    is_final: r.get::<_, i64>(6)? != 0,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, triggered_at, trigger_type, style, content, user_action FROM suggestions WHERE meeting_id = ? ORDER BY triggered_at"
        )?;
        let suggestions: Vec<SuggestionRow> = stmt
            .query_map([meeting_id], |r| {
                Ok(SuggestionRow {
                    id: r.get(0)?,
                    meeting_id: r.get(1)?,
                    triggered_at: r.get(2)?,
                    trigger_type: r.get(3)?,
                    style: r.get(4)?,
                    content: r.get(5)?,
                    user_action: r.get(6)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;

        Ok((meeting, transcripts, suggestions))
    }

    fn persist(&self, meeting_id: &str, markdown: &str) -> Result<()> {
        let conn = self.db.conn();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // Auto-increment version per meeting
        let next_version: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM minutes WHERE meeting_id = ?",
            [meeting_id],
            |r| r.get(0),
        )?;

        conn.execute(
            "INSERT INTO minutes (meeting_id, version, markdown, generated_at, model_used) VALUES (?, ?, ?, ?, ?)",
            params![meeting_id, next_version, markdown, now, "MiniMax-M2.7-highspeed"],
        )?;
        Ok(())
    }
}
