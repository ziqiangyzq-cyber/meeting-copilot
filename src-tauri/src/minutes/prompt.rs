use crate::db::models::{Meeting, SuggestionRow, TranscriptRow};
use std::fmt::Write;

const SYSTEM_PROMPT: &str = r#"你是用户的会议纪要生成助手。
用户刚开完一场会议(类型不定 — 工作 / 客户 / 评审 / 私人沟通都有可能)。
不要预设行业,根据会议元数据 + 转写内容判断场景。
你的任务:基于全场转写 + AI 给过的建议历史 + 会议元数据,生成一份结构化中文 Markdown 纪要。

## 你的产出格式(严格按以下结构,不要省略任何 ## 标题)

# {会议名}

**时间**: {开始 — 结束}({时长})
**关联项目**: {project_ref 或 "—"}
**会议目的**: {purpose 或 "—"}
**参会人**: {participants 或 "—"}

## 摘要
{3-5 句话讲清楚:这场会议讨论了什么、达成什么主要结论}

## 关键决议
{bulleted list,如果对话里没有明确决议就写 "(无明确决议)"}

## Action Items
{Markdown checklist,格式 `- [ ] **{谁}** — {做什么} (截止: {何时})`;如果没识别出 action items 就写 "(无)"}

## 风险与红线
{提到的报价底线、时间风险、合规疑虑、对方意图警示等;如果没有就写 "(无)"}

## 引用资料
{用到的会议前资料,格式 `- {文件名}: {何处/如何用到}`;如果整场会议没引用任何资料就写 "(无)"}

## 完整转写
<details>
<summary>展开</summary>

{逐句列出全场转写,格式 `**对方** {时间戳}: {内容}` 和 `**我** {时间戳}: {内容}`}

</details>

## 行文要求
- 用专业但简洁的中文,避免废话和总结性套话
- "对方"用更具体的称呼如果元数据有(如"陆家嘴林总")
- 数字、报价、日期、人名等关键信息原样保留
- 不要编造对话里没有的内容
- 全文 800-1500 字之间(完整转写不算)
"#;

pub struct MinutesContext<'a> {
    pub meeting: &'a Meeting,
    pub transcripts: &'a [TranscriptRow],
    pub suggestions: &'a [SuggestionRow],
}

pub fn system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

pub fn user_prompt(ctx: &MinutesContext) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "## 会议元数据");
    let _ = writeln!(out, "- 会议名: {}", ctx.meeting.name);
    let _ = writeln!(
        out,
        "- 关联项目: {}",
        ctx.meeting.project_ref.as_deref().unwrap_or("—")
    );
    let _ = writeln!(
        out,
        "- 会议目的: {}",
        ctx.meeting.purpose.as_deref().unwrap_or("—")
    );
    let _ = writeln!(
        out,
        "- 参会人: {}",
        ctx.meeting.participants.as_deref().unwrap_or("—")
    );
    if let Some(f) = &ctx.meeting.focus_points {
        if !f.trim().is_empty() {
            let _ = writeln!(out, "- 本次重点关注: {}", f.trim());
        }
    }
    let _ = writeln!(out, "- 开始: {}", fmt_ms(ctx.meeting.started_at));
    if let Some(end) = ctx.meeting.ended_at {
        let _ = writeln!(out, "- 结束: {}", fmt_ms(end));
        let _ = writeln!(
            out,
            "- 时长: {}",
            fmt_duration(end - ctx.meeting.started_at)
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## 全场转写(对方 = 系统音频, 我 = 麦克风)");
    if ctx.transcripts.is_empty() {
        let _ = writeln!(out, "(无)");
    } else {
        for t in ctx.transcripts {
            let speaker = match t.speaker.as_deref() {
                Some("system") => "对方",
                Some("mic") => "我",
                _ => "?",
            };
            let _ = writeln!(
                out,
                "[+{:.1}s] {}: {}",
                t.start_ms as f64 / 1000.0,
                speaker,
                t.text.trim()
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## AI 在会议中给过的建议(供你参考会议节奏)");
    if ctx.suggestions.is_empty() {
        let _ = writeln!(out, "(无)");
    } else {
        for s in ctx.suggestions {
            let _ = writeln!(out, "- [{}] {}", fmt_ms(s.triggered_at), s.content.trim());
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "---");
    let _ = writeln!(out, "请按上面\"产出格式\"生成完整纪要。");

    out
}

fn fmt_ms(ms: i64) -> String {
    use chrono::{TimeZone, Utc};
    if ms <= 0 {
        return "—".to_string();
    }
    let dt = Utc.timestamp_millis_opt(ms).single();
    match dt {
        Some(d) => {
            let local: chrono::DateTime<chrono::Local> = d.into();
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        None => format!("{ms} (raw ms)"),
    }
}

fn fmt_duration(ms: i64) -> String {
    let secs = ms / 1000;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::models::{Meeting, SuggestionRow, TranscriptRow};

    fn sample_meeting() -> Meeting {
        Meeting {
            id: "m1".into(),
            name: "陆家嘴连桥谈判".into(),
            project_ref: Some("陆家嘴连桥".into()),
            purpose: Some("报价谈判".into()),
            participants: Some("林总, 李工".into()),
            started_at: 1_700_000_000_000,
            ended_at: Some(1_700_003_600_000),
            audio_path: None,
            metadata: None,
            focus_points: None,
        }
    }

    fn sample_transcripts() -> Vec<TranscriptRow> {
        vec![
            TranscriptRow {
                id: 1,
                meeting_id: "m1".into(),
                speaker: Some("system".into()),
                text: "你们的报价比同行高 20%".into(),
                start_ms: 0,
                end_ms: 3000,
                is_final: true,
            },
            TranscriptRow {
                id: 2,
                meeting_id: "m1".into(),
                speaker: Some("mic".into()),
                text: "我们的服务范围更全".into(),
                start_ms: 4000,
                end_ms: 6500,
                is_final: true,
            },
        ]
    }

    #[test]
    fn user_prompt_includes_all_sections() {
        let m = sample_meeting();
        let ts = sample_transcripts();
        let ss: Vec<SuggestionRow> = vec![];
        let ctx = MinutesContext {
            meeting: &m,
            transcripts: &ts,
            suggestions: &ss,
        };
        let s = user_prompt(&ctx);
        assert!(s.contains("会议名: 陆家嘴连桥谈判"));
        assert!(s.contains("关联项目: 陆家嘴连桥"));
        assert!(s.contains("对方: 你们的报价比同行高"));
        assert!(s.contains("我: 我们的服务范围更全"));
        assert!(s.contains("产出格式"));
    }

    #[test]
    fn user_prompt_handles_empty_transcripts() {
        let m = sample_meeting();
        let ctx = MinutesContext {
            meeting: &m,
            transcripts: &[],
            suggestions: &[],
        };
        let s = user_prompt(&ctx);
        assert!(s.contains("全场转写"));
        // Should still have empty section marker
        assert!(s.matches("(无)").count() >= 2);
    }

    #[test]
    fn user_prompt_includes_focus_points() {
        let mut m = sample_meeting();
        m.focus_points = Some("拿到对方对交付时间的明确承诺".into());
        let ctx = MinutesContext {
            meeting: &m,
            transcripts: &[],
            suggestions: &[],
        };
        let s = user_prompt(&ctx);
        assert!(s.contains("本次重点关注: 拿到对方对交付时间的明确承诺"));
    }

    #[test]
    fn system_prompt_is_neutralized() {
        let s = system_prompt();
        assert!(s.contains("会议纪要生成助手"));
        assert!(!s.contains("Zion"));
        assert!(!s.contains("EFC 创羿幕墙"));
    }
}
