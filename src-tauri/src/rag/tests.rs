#[cfg(test)]
mod integration {
    use crate::db::Db;
    use crate::rag::{embedding::EmbeddingClient, ingest, retrieve};
    use rusqlite::params;
    use std::io::Write;

    fn make_fixture_md() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("测试报价单.md");
        // Long enough to produce multiple chunks (chunker target=500 chars).
        // Each section is padded to ~300+ chars so price / timeline / risk land
        // in distinct chunks and KNN ordering becomes meaningful.
        let content = r#"# 测试项目报价单 v3

## 服务范围说明

本项目提供 8 个阶段的全流程顾问服务,覆盖方案设计、扩初设计、施工图设计、招标配合、施工配合、竣工验收等全过程。我们的顾问团队由资深专家组成,覆盖结构、热工、声学、防水、防火等多个专业方向。服务内容包括方案评审、节点设计、计算分析、样板审核、招标技术文件编制、施工现场配合等。

## 报价与同行参考

合同总价 211 万元人民币,分 6 期支付,首付 30% 即 63.3 万元于合同生效后 7 个工作日内支付。后续各期按服务阶段完成情况支付。竞争对手 A 同类项目报价约 240 万元,竞争对手 B 设计咨询约 280 万元,竞争对手 C 工程咨询约 260 万元。本方报价比同行低约 10-20%,主要因为本地化运营,人力成本结构不同。如对报价有异议,可参考同行报价区间评估性价比。

## 关键时间节点

预计 7 个月完成扩初阶段交付,12 个月完成施工图设计交付。其中方案优化阶段 2 个月,扩初阶段 3 个月,施工图阶段 5 个月。各阶段交付物包括设计图纸、计算书、技术规格书。设计完成后进入招标配合阶段,预计第二年 Q1 开始施工配合,第二年 Q4 完成安装,第三年初竣工验收。

## 风险条款与终止机制

如业主单方面终止合同,已完成阶段按工作量结算,不退已收款项。如因服务方原因延期超过 30 天,业主有权要求扣减下一期款项 5%。不可抗力情况下双方协商处理。所有争议提交所在地仲裁委员会仲裁。本合同自双方签字盖章之日起生效。
"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    #[tokio::test]
    #[ignore = "requires ALIYUN_API_KEY env and network"]
    async fn ingest_and_retrieve_chinese_md() {
        let api_key = std::env::var("ALIYUN_API_KEY").expect("ALIYUN_API_KEY not set");

        // Open in-memory DB (sqlite-vec works with :memory: too)
        let tmp_db = tempfile::tempdir().unwrap();
        let db_path = tmp_db.path().join("test.sqlite");
        let db = Db::open(&db_path).unwrap();

        // Seed a meeting
        let meeting_id = "test-meeting-1";
        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO meetings (id, name, started_at) VALUES (?, ?, ?)",
                params![meeting_id, "测试会议", 0_i64],
            )
            .unwrap();
        }

        // Ingest the fixture
        let (_tmp_files, file_path) = make_fixture_md();
        let embed = EmbeddingClient::new(api_key);
        let material_id = ingest::ingest_file(&db, &embed, meeting_id, &file_path)
            .await
            .expect("ingest failed");

        println!("ingested material_id = {material_id}");

        // Verify DB state
        {
            let conn = db.conn();
            let chunk_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM chunks WHERE meeting_id = ?",
                    [meeting_id],
                    |r| r.get(0),
                )
                .unwrap();
            let vec_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM chunks_vec", [], |r| r.get(0))
                .unwrap();
            println!("chunks: {chunk_count}, chunks_vec: {vec_count}");
            assert!(chunk_count > 0, "should have chunks");
            assert_eq!(
                chunk_count, vec_count,
                "chunks_vec count should match chunks count"
            );
        }

        // Query 1: about pricing
        let q1 = "对方说报价太高怎么办";
        let r1 = retrieve::retrieve(&db, &embed, meeting_id, q1, 3)
            .await
            .expect("retrieve 1 failed");
        println!("\nQuery: {q1}");
        for c in &r1 {
            println!(
                "  [{:.4}] {}",
                c.distance,
                c.text.chars().take(80).collect::<String>()
            );
        }
        assert!(!r1.is_empty());
        let top1_text = &r1[0].text;
        // Top result should mention 211 万 / 报价 / 同行
        assert!(
            top1_text.contains("211") || top1_text.contains("报价") || top1_text.contains("同行"),
            "top-1 should be price-related, got: {top1_text}"
        );

        // Query 2: about timeline
        let q2 = "什么时候能完成设计";
        let r2 = retrieve::retrieve(&db, &embed, meeting_id, q2, 3)
            .await
            .expect("retrieve 2 failed");
        println!("\nQuery: {q2}");
        for c in &r2 {
            println!(
                "  [{:.4}] {}",
                c.distance,
                c.text.chars().take(80).collect::<String>()
            );
        }
        assert!(!r2.is_empty());
        let top1_text = &r2[0].text;
        assert!(
            top1_text.contains("施工图")
                || top1_text.contains("扩初")
                || top1_text.contains("阶段"),
            "top-1 should be timeline-related, got: {top1_text}"
        );
    }
}
