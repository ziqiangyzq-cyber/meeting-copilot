//! Meeting templates — hardcoded constants. Each template affects:
//!   1. Setup form prefill (会议目的 + focus_points placeholder)
//!   2. Minutes schema (the `##` section layout in user prompt)
//!
//! Templates do NOT change the realtime suggestion prompt — that stays
//! technical-meeting-default. Variations come from user-written focus_points.

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct MeetingTemplate {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_purpose: &'static str,
    pub focus_placeholder: &'static str,
    pub minutes_schema: &'static str,
}

pub const TEMPLATE_DEFAULT: MeetingTemplate = MeetingTemplate {
    id: "default",
    display_name: "默认(技术会议通用)",
    default_purpose: "",
    focus_placeholder: "开会前在这里写本次特别关注的技术点,AI 会围绕这些给提示。\n例:防火分区合规性 / 节点构造的耐久性 / 跟结构院的接口边界 / 核对图纸 vs 模型一致性",
    minutes_schema: r#"## 摘要
{3-5 句话讲清楚:这场会议讨论了什么技术议题,达成的主要结论}

## 技术决议
{bulleted list,本场会议明确决定的技术方案 / 方向 / 选型;如果对话里没有明确决议就写 "(无明确决议)"}

## 提疑与答疑
{Markdown 表格 `| 问题 | 提出方 | 回复方 | 结论 |`,捕获会议中讨论的技术问题及回应;如果没有就写 "(无)"}

## 技术风险与遗留
{识别的技术风险 / 未解决的设计问题 / 待跟进的边界条件;格式 `- {风险/遗留}: {建议或下一步}`;如果没有就写 "(无)"}

## Action Items
{Markdown checklist,格式 `- [ ] **{谁}** — {做什么} (截止: {何时})`;如果没识别到就写 "(无)"}

## 引用资料
{用到的会议前资料,格式 `- {文件名}: {何处/如何用到}`;如果整场会议没引用任何资料就写 "(无)"}

## 完整转写
<details>
<summary>展开</summary>

{逐句列出全场转写,格式 `**对方** {+N.Ns}: {内容}` 和 `**我** {+N.Ns}: {内容}`}

</details>"#,
};

pub const TEMPLATE_TECHNICAL_REVIEW: MeetingTemplate = MeetingTemplate {
    id: "technical_review",
    display_name: "技术评审",
    default_purpose: "技术评审",
    focus_placeholder: "本次重点核对的方案点 / 想提疑的设计问题 / 待确认的规范条文",
    minutes_schema: r#"## 摘要
{3-5 句话讲清楚:这场会议评审的对象 + 主要结论}

## 技术决议
{bulleted list:本场评审明确决定的技术方案 / 通过/不通过/有条件通过的项;如果没有就写 "(无明确决议)"}

## 提疑与答疑
{Markdown 表格 `| 问题 | 提出方 | 回复方 | 结论 |`,核心是会议中的提疑流程;如果没有就写 "(无)"}

## 修改要求
{bulleted list:评审中提出的具体修改点,格式 `- {对象}: {要怎么改}`;如果没有就写 "(无)"}

## 待澄清问题
{尚未达成结论 / 需进一步研究 / 跨方需确认的问题;格式 `- {问题}: {责任方 / 下次讨论时间}`;如果没有就写 "(无)"}

## Action Items
{Markdown checklist,格式 `- [ ] **{谁}** — {做什么} (截止: {何时})`;如果没识别到就写 "(无)"}

## 引用资料
{用到的会议前资料(规范/图纸/方案文本/历史项目),格式 `- {文件名}: {何处/如何用到}`;如果整场会议没引用任何资料就写 "(无)"}

## 完整转写
<details>
<summary>展开</summary>

{逐句列出全场转写}

</details>"#,
};

pub const TEMPLATE_COORDINATION: MeetingTemplate = MeetingTemplate {
    id: "coordination",
    display_name: "协调对接",
    default_purpose: "协调对接",
    focus_placeholder: "跟其他专业的接口边界 / 责任划分 / 关键时间节点 / 待提供的输入",
    minutes_schema: r#"## 摘要
{3-5 句话讲清楚:本次协调对接的范围 + 已经对齐的要点}

## 接口确认
{核心:各方负责什么 + 接口边界 + 已对齐的技术细节;格式 `- {接口/边界}: {责任方 + 决议}`;如果没有就写 "(无)"}

## 责任划分
{按方 / 按专业列清楚,谁负责什么模块/工作包;如果没有就写 "(无明确划分)"}

## 关键节点
{Markdown 表格 `| 日期 / 阶段 | 交付物 | 负责方 |`,排出双方/多方的关键时间表;如果没有就写 "(无)"}

## 风险与遗留
{识别的风险 / 未解决的边界问题 / 待跟进事项;格式 `- {风险/遗留}: {影响 + 下一步}`;如果没有就写 "(无)"}

## Action Items
{Markdown checklist,格式 `- [ ] **{谁}** — {做什么} (截止: {何时})`;如果没识别到就写 "(无)"}

## 引用资料
{用到的会议前资料,格式 `- {文件名}: {何处/如何用到}`;如果整场会议没引用任何资料就写 "(无)"}

## 完整转写
<details>
<summary>展开</summary>

{逐句列出全场转写}

</details>"#,
};

pub const TEMPLATE_FIELD_DISCUSSION: MeetingTemplate = MeetingTemplate {
    id: "field_discussion",
    display_name: "现场技术讨论",
    default_purpose: "现场技术讨论",
    focus_placeholder: "现场发现的问题 / 现场约束(尺寸/材料/施工条件)/ 整改时限",
    minutes_schema: r#"## 摘要
{3-5 句话讲清楚:本次现场讨论的位置 + 主要问题 + 已得出的结论}

## 问题清单
{Markdown 表格 `| 位置 | 问题描述 | 严重度 (低/中/高) |`;如果没有就写 "(无)"}

## 根因分析
{对每个主要问题分析根因,格式 `- {问题}: {根因推断}`;如果没有就写 "(无)"}

## 对策与整改
{每个问题对应的方案,格式 `- {问题}: {对策}`;如果没有就写 "(无)"}

## 责任与截止
{Markdown 表格 `| 责任人 | 整改任务 | 整改截止 | 复查日期 |`;如果没有就写 "(无)"}

## Action Items
{Markdown checklist,格式 `- [ ] **{谁}** — {做什么} (截止: {何时})`;如果没识别到就写 "(无)"}

## 引用资料
{用到的会议前资料,格式 `- {文件名}: {何处/如何用到}`;如果整场会议没引用任何资料就写 "(无)"}

## 完整转写
<details>
<summary>展开</summary>

{逐句列出全场转写}

</details>"#,
};

pub fn all_templates() -> Vec<MeetingTemplate> {
    vec![
        TEMPLATE_DEFAULT,
        TEMPLATE_TECHNICAL_REVIEW,
        TEMPLATE_COORDINATION,
        TEMPLATE_FIELD_DISCUSSION,
    ]
}

pub fn get_by_id(id: &str) -> MeetingTemplate {
    match id {
        "technical_review" => TEMPLATE_TECHNICAL_REVIEW,
        "coordination" => TEMPLATE_COORDINATION,
        "field_discussion" => TEMPLATE_FIELD_DISCUSSION,
        _ => TEMPLATE_DEFAULT,
    }
}
