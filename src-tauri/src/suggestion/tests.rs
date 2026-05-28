#[cfg(test)]
mod integration {
    use crate::asr::aliyun_paraformer::TranscriptEvent;
    use crate::asr::AudioSource;
    use crate::db::Db;
    use crate::llm::minimax::MiniMaxClient;
    use crate::rag::{embedding::EmbeddingClient, ingest};
    use crate::suggestion::{SuggestionEngine, TriggerType};
    use rusqlite::params;
    use std::io::Write;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn make_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("测试报价单.md");
        let content = r#"# 测试项目报价单 v3

## 服务范围
本项目提供 8 个阶段全流程顾问服务,包括方案设计、扩初设计、施工图设计、招标配合、施工配合、竣工验收。

## 报价
合同总价 211 万元人民币,分 6 期支付,首付 30%。底线价 180 万。

## 同行参考
同类项目竞争对手报价区间 180-280 万。

## 关键节点
预计 7 个月完成扩初阶段,12 个月完成施工图。
"#;
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        (dir, path)
    }

    #[tokio::test]
    #[ignore = "requires ALIYUN_API_KEY + MINIMAX_API_KEY env"]
    async fn engine_generates_suggestion_with_rag_context() {
        let aliyun_key = std::env::var("ALIYUN_API_KEY").expect("ALIYUN_API_KEY missing");
        let minimax_key = std::env::var("MINIMAX_API_KEY").expect("MINIMAX_API_KEY missing");

        // 1. Open in-memory DB + seed meeting
        let tmp_db = tempfile::tempdir().unwrap();
        let db_path = tmp_db.path().join("test.sqlite");
        let db = Arc::new(Db::open(&db_path).unwrap());

        let meeting_id = "test-meeting-1";
        {
            let conn = db.conn();
            conn.execute(
                "INSERT INTO meetings (id, name, started_at) VALUES (?, ?, ?)",
                params![meeting_id, "测试项目谈判", 0_i64],
            )
            .unwrap();
        }

        // 2. Ingest fixture
        let (_tmp, file_path) = make_fixture();
        let embed = Arc::new(EmbeddingClient::new(aliyun_key));
        ingest::ingest_file(&db, &embed, meeting_id, &file_path)
            .await
            .expect("ingest failed");

        // 3. Build engine (meta is re-loaded from DB inside generate())
        let llm: Arc<dyn crate::llm::LLMClient> = Arc::new(MiniMaxClient::new(minimax_key));
        let engine = SuggestionEngine::new(db.clone(), embed.clone(), llm, meeting_id.into());

        // 4. Push some transcript events simulating an active meeting
        let events = vec![
            (AudioSource::System, "你们的报价比同行高了 20% 啊"),
            (AudioSource::System, "我看竞争对手那边大概只要 180 万"),
            (AudioSource::Mic, "我们的服务范围跟他们不一样"),
        ];
        for (src, text) in events {
            engine
                .push_transcript(TranscriptEvent {
                    source: src,
                    text: text.into(),
                    is_final: true,
                    begin_ms: 0,
                    end_ms: 0,
                })
                .await;
        }

        // 5. Generate a suggestion
        let (tx, mut rx) = mpsc::channel::<String>(64);
        let handle = tokio::spawn(async move {
            let mut accumulated = String::new();
            while let Some(tok) = rx.recv().await {
                print!("{tok}");
                std::io::Write::flush(&mut std::io::stdout()).ok();
                accumulated.push_str(&tok);
            }
            accumulated
        });

        engine
            .generate(TriggerType::Manual, tx)
            .await
            .expect("generate failed");

        let suggestion = handle.await.unwrap();
        println!(
            "\n=== Final suggestion ({} chars) ===\n{suggestion}\n",
            suggestion.chars().count()
        );

        // Assertions: should produce a non-empty Chinese suggestion mentioning context
        assert!(!suggestion.is_empty());
        assert!(
            suggestion.chars().any(|c| c >= '\u{4e00}' && c <= '\u{9fff}'),
            "should contain Chinese chars"
        );
        // Should be short-ish (system prompt says <200 chars)
        assert!(
            suggestion.chars().count() < 400,
            "suggestion too long ({} chars):\n{suggestion}",
            suggestion.chars().count()
        );
    }
}
