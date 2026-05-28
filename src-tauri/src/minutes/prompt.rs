use crate::db::models::{Meeting, SuggestionRow, TranscriptRow};
use crate::templates::MeetingTemplate;
use std::fmt::Write;

const SYSTEM_PROMPT: &str = r#"你是用户的会议纪要生成助手。用户刚开完一场会议,大多是技术沟通场景(方案评审 / 跨专业协调 / 现场问题 / 技术答疑 / 图纸会审 / 内部培训等)。
不要预设行业,根据元数据 + 转写内容判断场景。

## 通用文头(产出开头固定按此格式)

# {会议名}

**时间**: {开始 — 结束}({时长})
**关联项目**: {project_ref 或 "—"}
**会议目的**: {purpose 或 "—"}
**参会人**: {participants 或 "—"}
**重点关注**: {focus_points 或 "—"}
**用户笔记**: {notes 或 "—"}

正文章节结构由用户消息中的「## 产出格式」决定 — 严格按那个结构走,不要省略任何 ## 标题。

## 行文要求
- 专业但简洁的中文,避免废话和总结性套话
- 数字、技术参数、规范号、人名等关键信息原样保留
- 不要编造对话里没有的内容
- "对方"用更具体的称呼如果元数据有(比如"陆家嘴林总" / "结构院李工")
- **如果用户有"重点关注"或"用户笔记",围绕这些点展开,不能忽视它们**
- 全文 800-1500 字之间(完整转写不算)

## 切换商务模板的情形

只有当元数据中"会议目的"明确包含"谈判 / 报价 / 立项 / 合同"等商务关键词时,可以追加额外章节:
- ## 谈判进展 / 让步点
- ## 对方意图分析

默认情况下不输出这些章节。
"#;

pub struct MinutesContext<'a> {
    pub meeting: &'a Meeting,
    pub transcripts: &'a [TranscriptRow],
    pub suggestions: &'a [SuggestionRow],
}

pub fn system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

pub fn user_prompt(ctx: &MinutesContext, template: &MeetingTemplate) -> String {
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
    if let Some(notes) = &ctx.meeting.notes {
        if !notes.trim().is_empty() {
            let _ = writeln!(
                out,
                "- 用户开会期间的快速笔记(以这些为锚点,纪要要围绕这些展开):"
            );
            let indented = notes.trim().replace('\n', "\n  ");
            let _ = writeln!(out, "  {}", indented);
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
    let _ = writeln!(out, "## 产出格式");
    let _ = writeln!(out);
    let _ = writeln!(out, "{}", template.minutes_schema);
    let _ = writeln!(out);
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
            notes: None,
            template_id: None,
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
        let s = user_prompt(&ctx, &crate::templates::TEMPLATE_DEFAULT);
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
        let s = user_prompt(&ctx, &crate::templates::TEMPLATE_DEFAULT);
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
        let s = user_prompt(&ctx, &crate::templates::TEMPLATE_DEFAULT);
        assert!(s.contains("本次重点关注: 拿到对方对交付时间的明确承诺"));
    }

    #[test]
    fn user_prompt_includes_notes_when_present() {
        let mut m = sample_meeting();
        m.notes = Some("对方接受 211 万\n下周三 demo".into());
        let ctx = MinutesContext {
            meeting: &m,
            transcripts: &[],
            suggestions: &[],
        };
        let s = user_prompt(&ctx, &crate::templates::TEMPLATE_DEFAULT);
        assert!(s.contains("用户开会期间的快速笔记"));
        assert!(s.contains("对方接受 211 万"));
        assert!(s.contains("下周三 demo"));
    }

    #[test]
    fn user_prompt_skips_notes_when_empty() {
        let mut m = sample_meeting();
        m.notes = Some("   ".into());
        let ctx = MinutesContext {
            meeting: &m,
            transcripts: &[],
            suggestions: &[],
        };
        let s = user_prompt(&ctx, &crate::templates::TEMPLATE_DEFAULT);
        assert!(!s.contains("用户开会期间的快速笔记"));
    }

    #[test]
    fn system_prompt_is_neutralized() {
        let s = system_prompt();
        assert!(!s.contains("Zion"));
        assert!(!s.contains("幕墙"));
        assert!(!s.contains("EFC"));
        // Make sure it's technical-meeting-oriented
        assert!(s.contains("技术会议") || s.contains("方案评审") || s.contains("技术沟通"));
    }
}
