#[cfg(test)]
mod integration_tests {
    use crate::asr::aliyun_paraformer::{AliyunParaformer, TranscriptEvent};
    use crate::asr::{ASRClient, AudioSource};
    use std::time::Duration;
    use tokio::sync::mpsc;

    #[tokio::test]
    #[ignore = "requires ALIYUN_API_KEY env and network"]
    async fn connect_and_get_chinese_transcript() {
        let key = std::env::var("ALIYUN_API_KEY").expect("ALIYUN_API_KEY not set");

        let (tx, mut rx) = mpsc::channel::<TranscriptEvent>(64);
        let mut client = AliyunParaformer::connect(key, None, tx)
            .await
            .expect("connect failed");

        // Read fixture (16kHz mono int16 LE WAV)
        let wav_path = "../tests/fixtures/chinese_30s.wav";
        let wav = std::fs::read(wav_path)
            .expect("fixture not found — run say + ffmpeg to generate");

        // Skip 44-byte WAV header (standard PCM WAV)
        let pcm = wav[44..].to_vec();

        // Send PCM in 100ms chunks (3200 bytes at 16kHz int16 mono)
        let chunk_size = 3200;
        let send_task = tokio::spawn(async move {
            for chunk in pcm.chunks(chunk_size) {
                if let Err(e) = client.push_pcm(AudioSource::System, chunk).await {
                    eprintln!("push_pcm error: {e}");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
            // Slight delay before closing so server can flush
            tokio::time::sleep(Duration::from_millis(500)).await;
            client.close().await.unwrap();
        });

        // Collect transcripts for up to 20 seconds
        let collect_task = tokio::spawn(async move {
            let mut text = String::new();
            let mut finals = 0;
            let deadline = tokio::time::sleep(Duration::from_secs(20));
            tokio::pin!(deadline);

            loop {
                tokio::select! {
                    maybe_evt = rx.recv() => {
                        match maybe_evt {
                            Some(evt) => {
                                println!("[asr][{:?}] is_final={} {}", evt.source, evt.is_final, evt.text);
                                if evt.is_final {
                                    text.push_str(&evt.text);
                                    finals += 1;
                                }
                            }
                            None => break,
                        }
                    }
                    _ = &mut deadline => break,
                }
            }
            (text, finals)
        });

        let _ = send_task.await;
        let (collected_text, final_count) = collect_task.await.unwrap();

        println!("\n=== FINAL TRANSCRIPT ===\n{collected_text}\n");
        println!("Final sentence count: {final_count}");

        assert!(
            !collected_text.is_empty(),
            "should get at least one transcript"
        );
        assert!(final_count > 0, "should get at least one final sentence");
        // The Chinese fixture mentions 陆家嘴 — check it's in there somewhere
        assert!(
            collected_text.contains("陆家嘴") || collected_text.contains("陸家嘴"),
            "transcript should contain 陆家嘴, got: {collected_text}"
        );
    }
}
