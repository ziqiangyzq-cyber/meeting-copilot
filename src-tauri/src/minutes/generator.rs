use crate::db::models::{Meeting, TranscriptRow};
use crate::db::Db;
use crate::error::{AppError, Result};
use crate::llm::{LLMClient, Message};
use crate::minutes::prompt::{system_prompt, user_prompt, MinutesContext};
use rusqlite::params;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Output budget for minutes generation. Covers the reasoning model's hidden
/// chain-of-thought plus a long-form document (a merged multi-segment meeting
/// can easily produce several thousand visible tokens).
const MINUTES_MAX_TOKENS: u32 = 16384;

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
        // Load meeting + transcripts (suggestions deliberately excluded from
        // minutes — they are in-meeting aids, persisted only for history view)
        let (meeting, transcripts) = self.load_context(meeting_id)?;

        let template = crate::templates::get_by_id(
            meeting.template_id.as_deref().unwrap_or("default"),
        );

        let ctx = MinutesContext {
            meeting: &meeting,
            transcripts: &transcripts,
        };

        let system = system_prompt();
        let user = user_prompt(&ctx, &template);

        self.stream_and_persist(system, user, out, meeting_id).await
    }

    /// Generate a single merged minutes from several meeting records (e.g. a
    /// meeting that was interrupted and restarted into separate records). The
    /// segments are ordered chronologically by `started_at`, their transcripts
    /// concatenated with monotonically increasing timestamps, and the result is
    /// persisted under the earliest meeting's id.
    pub async fn generate_merged(
        &self,
        meeting_ids: &[String],
        out: mpsc::Sender<String>,
    ) -> Result<String> {
        if meeting_ids.is_empty() {
            return Err(AppError::Asr("no meetings selected for merge".into()));
        }
        if meeting_ids.len() == 1 {
            return self.generate(&meeting_ids[0], out).await;
        }

        // Load every segment, then order chronologically regardless of the
        // order they were selected in the UI.
        let mut loaded = Vec::with_capacity(meeting_ids.len());
        for mid in meeting_ids {
            loaded.push(self.load_context(mid)?);
        }
        loaded.sort_by_key(|(m, _)| m.started_at);

        let base_meeting = loaded[0].0.clone();
        let mut all_transcripts: Vec<TranscriptRow> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut time_offset: i64 = 0;
        for (meeting, transcripts) in &loaded {
            // Dedupe consecutive identical segment names for the title.
            if names.last() != Some(&meeting.name) {
                names.push(meeting.name.clone());
            }
            let seg_max = transcripts.iter().map(|t| t.end_ms).max().unwrap_or(0);
            for t in transcripts {
                let mut t2 = t.clone();
                t2.start_ms += time_offset;
                t2.end_ms += time_offset;
                all_transcripts.push(t2);
            }
            time_offset += seg_max + 1;
        }

        // Synthesize a merged meeting so the prompt reflects the combined record.
        let mut merged_meeting = base_meeting.clone();
        merged_meeting.name = format!("{}(合并 {} 段记录)", names.join(" + "), loaded.len());

        let template = crate::templates::get_by_id(
            merged_meeting.template_id.as_deref().unwrap_or("default"),
        );
        let ctx = MinutesContext {
            meeting: &merged_meeting,
            transcripts: &all_transcripts,
        };
        let system = system_prompt();
        let user = user_prompt(&ctx, &template);

        // Persist under the earliest segment's id so it surfaces in that meeting.
        self.stream_and_persist(system, user, out, &base_meeting.id)
            .await
    }

    /// Shared LLM streaming + versioned persistence used by both single and
    /// merged generation.
    async fn stream_and_persist(
        &self,
        system: &str,
        user: String,
        out: mpsc::Sender<String>,
        persist_meeting_id: &str,
    ) -> Result<String> {
        let (tx, mut rx) = mpsc::channel::<String>(256);
        let llm = self.llm.clone();
        let messages = vec![Message::system(system), Message::user(user)];
        // Minutes are long-form output and the reasoning model spends hidden
        // chain-of-thought from the same budget — the default 1024 used to
        // truncate minutes (especially merged ones) mid-document.
        let llm_task =
            tokio::spawn(async move { llm.stream_max(messages, tx, MINUTES_MAX_TOKENS).await });

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
        self.persist(persist_meeting_id, &markdown)?;

        Ok(markdown)
    }

    fn load_context(
        &self,
        meeting_id: &str,
    ) -> Result<(Meeting, Vec<TranscriptRow>)> {
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

        Ok((meeting, transcripts))
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
