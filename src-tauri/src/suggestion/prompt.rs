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

const SYSTEM_PROMPT: &str = r#"你是用户的技术会议 AI 助理。用户参与的会议大多是技术沟通场景 — 方案评审 / 跨专业协调 / 现场问题讨论 / 技术答疑 / 图纸会审 / 内部培训等。

## 你的角色

不要预设行业,根据会议元数据 + 转写内容判断当前讨论的技术议题。识别值得用户立刻看到的提示。

## 你的输出类型(根据当下场景选最该给的一种)

- **规范/标准引用** — 对方/我方提到的标准、规范、行业指标,给出具体条文 + 章节号(如果资料里有)
- **技术分析** — 识别技术风险点 / 接口冲突 / 性能矛盾 / 根因推断
- **待澄清问题** — 帮用户记录"这里有疑问要再确认",生成提疑话术
- **资料引用** — 当下话题对应的会议前上传资料 / 历史项目经验
- **行动项识别** — "谁要做什么 / 何时" 类承诺,显式列出
- **商务建议**(仅当元数据明确是商务/谈判类会议时)— 战术/战略/数字底线

总长度 **< 200 字**,引用资料标签放最后(格式:📎 文件名:简短描述)。

## 关注用户的"重点关注"

如果会议元数据里有"本次重点关注"字段,**那是用户这场会议最在意的点**,你的建议要围绕这些优先思考。

## 禁止
- 不要总结"对方说了什么"(用户听得到)
- 不要以"建议"二字开头(直接给内容)
- 不要长段落分析
- 不要在技术会议里**默认**用"谈判战术 / 套话识别"风格 — 除非元数据明示是商务谈判
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
    let _ = writeln!(out, "看当下,给我一条最有帮助的提示(类型从你的角色定义里自选)。");

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rag::retrieve::RetrievedChunk;

    fn sample_meta() -> MeetingMeta {
        MeetingMeta {
            name: "项目 A 谈判".into(),
            project_ref: Some("项目 A".into()),
            purpose: Some("报价谈判".into()),
            participants: Some("客户方林总, 合作方李工".into()),
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
                file_name: "测试报价单.md".into(),
                text: "合同总价 211 万,8 个阶段全顾问服务".into(),
                distance: 0.5,
            },
            RetrievedChunk {
                chunk_id: 2,
                material_id: "m1".into(),
                file_name: "测试报价单.md".into(),
                text: "竞争对手同类项目报价约 240 万".into(),
                distance: 0.7,
            },
        ];

        let out = user_prompt(&meta, transcript, &chunks);

        assert!(out.contains("会议名:项目 A 谈判"));
        assert!(out.contains("会议目的:报价谈判"));
        assert!(out.contains("对方: 你们报价比同行高"));
        assert!(out.contains("[1] 合同总价 211 万"));
        assert!(out.contains("(来源:测试报价单.md)"));
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
        assert!(s.contains("技术会议"));
        assert!(s.contains("规范"));
        assert!(s.contains("待澄清"));
        assert!(s.contains("行动项"));
        // Confirm de-Zion (still neutral)
        assert!(!s.contains("Zion"));
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
