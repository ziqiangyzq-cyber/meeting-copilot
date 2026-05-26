use crate::rag::retrieve::RetrievedChunk;
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct MeetingMeta {
    pub name: String,
    pub project_ref: Option<String>,
    pub purpose: Option<String>,
    pub participants: Option<String>,
}

const SYSTEM_PROMPT: &str = r#"你是 EFC 创羿幕墙顾问公司合伙人杨自强的会议 AI 助理。
他在跟客户/建筑师/总包谈判或评审,你帮他做下一步决策辅助。

## 你的输出风格(智能混合)

根据当下对话节奏,从三种风格中选最该给的:
- **战术**(对方刚说一句你不知怎么接)→ 给一句能直接说的话术 + 1 句简短佐证
- **战略**(对方反复绕一个话题 / 在套话)→ 分析对方意图 + 应对方向(具体话他自己说)
- **信息**(对方问数字 / 引规范 / 引项目)→ 直接摆事实数据 + 来源

无论哪种风格,**总长度 < 200 字**,引用资料标签放最后(格式:📎 文件名:简短描述)。

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
        assert!(s.contains("EFC"));
        assert!(s.contains("智能混合"));
        assert!(s.contains("战术"));
        assert!(s.contains("战略"));
        assert!(s.contains("信息"));
    }
}
