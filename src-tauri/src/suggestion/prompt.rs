use crate::rag::retrieve::RetrievedChunk;
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct MeetingMeta {
    pub name: String,
    pub project_ref: Option<String>,
    pub purpose: Option<String>,
    pub participants: Option<String>,
    pub focus_points: Option<String>,
}

const SYSTEM_PROMPT: &str = r#"你是用户的会议 AI 助理。用户正在开一场会议(类型不定:可能是工作会议 / 客户谈判 / 项目评审 / 内部讨论 / 私人沟通,完全看会议元数据)。

## 你的角色

你不是只能聊幕墙建筑或某个特定行业 — 你根据当下的会议元数据 + 转写内容判断场景,给出合适的建议。不要预设行业。

## 你的输出风格(智能混合)

根据当下对话节奏,从三种风格中选最该给的:
- **战术**(对方刚说一句你不知怎么接)→ 给一句能直接说的话术 + 1 句简短佐证
- **战略**(对方反复绕一个话题 / 在套话)→ 分析对方意图 + 应对方向(具体话他自己说)
- **信息**(对方问数字 / 引规范 / 引项目)→ 直接摆事实数据 + 来源

无论哪种风格,**总长度 < 200 字**,引用资料标签放最后(格式:📎 文件名:简短描述)。

## 关注用户的"重点关注"

如果会议元数据里有"本次重点关注"字段,**那就是用户这场会议最在意的事**。你的建议应该围绕这些点优先思考。

## 禁止
- 不要总结"对方说了什么"(他听得到)
- 不要以"建议"二字开头(直接给内容)
- 不要长段落分析
- 中英文混说没问题但别炫
"#;

pub fn system_prompt() -> &'static str {
    SYSTEM_PROMPT
}

pub fn user_prompt(
    meta: &MeetingMeta,
    recent_transcript: &str,
    chunks: &[RetrievedChunk],
) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "## 会议元数据");
    let _ = writeln!(out, "会议名:{}", meta.name);
    if let Some(p) = &meta.project_ref {
        let _ = writeln!(out, "关联项目:{p}");
    }
    if let Some(p) = &meta.purpose {
        let _ = writeln!(out, "会议目的:{p}");
    }
    if let Some(p) = &meta.participants {
        let _ = writeln!(out, "参会人:{p}");
    }
    if let Some(f) = &meta.focus_points {
        if !f.trim().is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "## 本次重点关注(用户临时设的,你的建议要围绕这个)");
            let _ = writeln!(out, "{}", f.trim());
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## 最近转写片段(对方=系统音频,我=麦克风)");
    if recent_transcript.trim().is_empty() {
        let _ = writeln!(out, "(无转写)");
    } else {
        let _ = writeln!(out, "{recent_transcript}");
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## 相关资料(检索 top-{})", chunks.len());
    if chunks.is_empty() {
        let _ = writeln!(out, "(无)");
    } else {
        for (i, c) in chunks.iter().enumerate() {
            let _ = writeln!(out, "[{}] {} (来源:{})", i + 1, c.text.trim(), c.file_name);
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## 任务");
    let _ = writeln!(out, "看当下,给我一条建议(战术/战略/信息 自选)。");

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::retrieve::RetrievedChunk;

    fn sample_meta() -> MeetingMeta {
        MeetingMeta {
            name: "陆家嘴连桥谈判".into(),
            project_ref: Some("陆家嘴连桥".into()),
            purpose: Some("报价谈判".into()),
            participants: Some("陆家嘴林总, 华东院李工".into()),
            focus_points: None,
        }
    }

    #[test]
    fn user_prompt_with_chunks() {
        let meta = sample_meta();
        let transcript = "对方: 你们报价比同行高 20%\n我: 我们的服务范围更全\n";
        let chunks = vec![
            RetrievedChunk {
                chunk_id: 1,
                material_id: "m1".into(),
                file_name: "陆家嘴报价单.md".into(),
                text: "合同总价 211 万,8 个阶段全顾问服务".into(),
                distance: 0.5,
            },
            RetrievedChunk {
                chunk_id: 2,
                material_id: "m1".into(),
                file_name: "陆家嘴报价单.md".into(),
                text: "KPF 顾问同类项目报价约 240 万".into(),
                distance: 0.7,
            },
        ];

        let out = user_prompt(&meta, transcript, &chunks);

        assert!(out.contains("会议名:陆家嘴连桥谈判"));
        assert!(out.contains("会议目的:报价谈判"));
        assert!(out.contains("对方: 你们报价比同行高"));
        assert!(out.contains("[1] 合同总价 211 万"));
        assert!(out.contains("(来源:陆家嘴报价单.md)"));
        assert!(out.contains("看当下"));
    }

    #[test]
    fn user_prompt_no_chunks() {
        let meta = sample_meta();
        let out = user_prompt(&meta, "", &[]);
        assert!(out.contains("(无转写)"));
        assert!(out.contains("(无)"));
    }

    #[test]
    fn system_prompt_contains_role() {
        let s = system_prompt();
        assert!(s.contains("会议 AI 助理"));
        assert!(!s.contains("Zion"));
        assert!(s.contains("智能混合"));
        assert!(s.contains("战术"));
        assert!(s.contains("战略"));
        assert!(s.contains("信息"));
        // Should NOT hardcode industry
        assert!(!s.contains("EFC 创羿"));
        // Should mention focus_points handling
        assert!(s.contains("重点关注"));
    }

    #[test]
    fn user_prompt_includes_focus_points_when_set() {
        let meta = MeetingMeta {
            name: "周一例会".into(),
            project_ref: None,
            purpose: None,
            participants: None,
            focus_points: Some("对方今天要砍价,留意话术".into()),
        };
        let out = user_prompt(&meta, "对方:能不能便宜一点", &[]);
        assert!(out.contains("本次重点关注"));
        assert!(out.contains("对方今天要砍价,留意话术"));
    }

    #[test]
    fn user_prompt_skips_focus_points_when_empty() {
        let meta = MeetingMeta {
            name: "周一例会".into(),
            project_ref: None,
            purpose: None,
            participants: None,
            focus_points: Some("   ".into()),
        };
        let out = user_prompt(&meta, "", &[]);
        assert!(!out.contains("本次重点关注"));
    }
}
