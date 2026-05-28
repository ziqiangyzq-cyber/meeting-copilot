use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};

const EMBED_URL: &str = "https://dashscope.aliyuncs.com/api/v1/services/embeddings/text-embedding/text-embedding";
const MODEL: &str = "text-embedding-v3";
const DIMENSION: u32 = 1024;
const MAX_BATCH: usize = 25;

#[derive(Serialize)]
struct EmbedReq<'a> {
    model: &'a str,
    input: EmbedInput<'a>,
    parameters: EmbedParams,
}

#[derive(Serialize)]
struct EmbedInput<'a> {
    texts: &'a [String],
}

#[derive(Serialize)]
struct EmbedParams {
    dimension: u32,
}

#[derive(Deserialize, Debug)]
struct EmbedResp {
    output: Option<EmbedOutput>,
    code: Option<String>,
    message: Option<String>,
}

#[derive(Deserialize, Debug)]
struct EmbedOutput {
    embeddings: Vec<EmbedItem>,
}

#[derive(Deserialize, Debug)]
struct EmbedItem {
    text_index: usize,
    embedding: Vec<f32>,
}

pub struct EmbeddingClient {
    api_key: String,
    client: reqwest::Client,
}

impl EmbeddingClient {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("reqwest client build");
        Self { api_key, client }
    }

    /// Embed `texts` in batches of up to 25. Returns embeddings in input order.
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut all = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(MAX_BATCH) {
            let batch = self.embed_one_batch(chunk).await?;
            all.extend(batch);
        }
        Ok(all)
    }

    async fn embed_one_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let req_body = EmbedReq {
            model: MODEL,
            input: EmbedInput { texts },
            parameters: EmbedParams { dimension: DIMENSION },
        };

        let resp = self
            .client
            .post(EMBED_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| AppError::Asr(format!("embedding request failed: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Asr(format!("embedding read body failed: {e}")))?;

        if !status.is_success() {
            return Err(AppError::Asr(format!("embedding HTTP {status}: {body}")));
        }

        let parsed: EmbedResp = serde_json::from_str(&body)?;

        if let Some(out) = parsed.output {
            // Sort by text_index just in case the server returns out of order
            let mut items = out.embeddings;
            items.sort_by_key(|i| i.text_index);
            Ok(items.into_iter().map(|i| i.embedding).collect())
        } else {
            Err(AppError::Asr(format!(
                "embedding error: code={:?} msg={:?}",
                parsed.code, parsed.message
            )))
        }
    }
}

/// Cosine similarity between two equal-length f32 vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_basic() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-6);
        assert!(cosine(&a, &c).abs() < 1e-6);
    }

    #[tokio::test]
    #[ignore = "requires ALIYUN_API_KEY env and network"]
    async fn embed_chinese_sentences_real() {
        let key = std::env::var("ALIYUN_API_KEY").expect("ALIYUN_API_KEY not set");
        let client = EmbeddingClient::new(key);

        // test that semantically similar texts have higher cosine sim than unrelated ones:
        //   0: 项目预算估算(product topic)
        //   1: 产品功能模块(same product topic, different wording)
        //   2: 今天天气真好(unrelated topic)
        let texts = vec![
            "项目 A 的预算估算约 211 万".to_string(),
            "我们的产品涵盖 8 大功能模块".to_string(),
            "今天的天气很好,我想去公园散步".to_string(),
        ];

        let vecs = client.embed_batch(&texts).await.expect("embedding failed");

        assert_eq!(vecs.len(), 3, "should get 3 embeddings");
        for (i, v) in vecs.iter().enumerate() {
            assert_eq!(v.len(), 1024, "embedding {i} should be 1024-dim, got {}", v.len());
        }

        let sim_business = cosine(&vecs[0], &vecs[1]);
        let sim_unrelated = cosine(&vecs[0], &vecs[2]);

        println!("similarity(预算估算, 产品功能)   = {sim_business:.4}");
        println!("similarity(预算估算, 天气真好)    = {sim_unrelated:.4}");

        // Business-related sentences should be more similar than unrelated ones.
        // Don't assert specific values (they depend on the model); just relative ordering.
        assert!(
            sim_business > sim_unrelated,
            "business pair (sim={sim_business}) should be more similar than unrelated pair (sim={sim_unrelated})"
        );
    }
}
