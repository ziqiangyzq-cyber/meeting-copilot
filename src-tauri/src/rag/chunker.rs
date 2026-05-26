/// Split `text` into sentence-aware chunks of approximately `target` characters,
/// with the last `overlap` characters of each chunk prepended to the next.
///
/// - Sentence boundaries: `。`, `！`, `？`, `.`, `!`, `?`, and newlines
/// - "character" means Unicode scalar (chars().count())
/// - Empty text yields empty Vec
/// - A sentence longer than `target` is emitted as its own chunk
pub fn chunk(text: &str, target: usize, overlap: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if target == 0 {
        return vec![trimmed.to_string()];
    }

    // Step 1: split into sentences
    let sentences = split_sentences(trimmed);

    // Step 2: greedy pack
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;

    for sent in sentences {
        let sent_len = sent.chars().count();

        // If adding this sentence would exceed target, emit current chunk and start fresh
        if current_len > 0 && current_len + sent_len > target {
            chunks.push(current.clone());
            current = tail_overlap(&current, overlap);
            current_len = current.chars().count();
        }
        current.push_str(&sent);
        current_len += sent_len;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

/// Split text into sentences, keeping the terminator with each sentence.
/// Sentences are returned with no leading whitespace but may include trailing
/// whitespace from the original text (preserves natural boundaries).
fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();

    for c in text.chars() {
        current.push(c);
        if is_sentence_terminator(c) {
            // Emit current as a sentence; skip if it's only whitespace + terminator
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                sentences.push(current.clone());
            }
            current.clear();
        }
    }

    // Trailing text without a terminator
    let leftover = current.trim();
    if !leftover.is_empty() {
        sentences.push(current);
    }

    sentences
}

fn is_sentence_terminator(c: char) -> bool {
    matches!(c, '。' | '！' | '？' | '.' | '!' | '?' | '\n')
}

/// Return the last `n` chars of `s` as a new String. If `s` has fewer than `n`
/// chars, returns all of `s`.
fn tail_overlap(s: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let total = s.chars().count();
    if total <= n {
        return s.to_string();
    }
    let skip = total - n;
    s.chars().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_empty() {
        assert_eq!(chunk("", 500, 50), Vec::<String>::new());
        assert_eq!(chunk("   \n  \t  ", 500, 50), Vec::<String>::new());
    }

    #[test]
    fn short_text_one_chunk() {
        let text = "这是一个很短的中文测试。 hello world.";
        let chunks = chunk(text, 500, 50);
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].contains("中文测试"));
        assert!(chunks[0].contains("hello"));
    }

    #[test]
    fn long_text_multiple_chunks_with_overlap() {
        // Generate ~2000 chars of Chinese text with sentence boundaries every ~50 chars
        let sentence = "陆家嘴连桥项目报价方案要重新审视。"; // 18 chars
        let text: String = std::iter::repeat(sentence).take(120).collect(); // ~2160 chars
        let chunks = chunk(&text, 500, 50);

        // Expect 4-6 chunks (target=500, total=2160; with overlap each chunk after the first
        // starts with 50 chars of prior context)
        assert!(
            chunks.len() >= 4 && chunks.len() <= 6,
            "expected 4-6 chunks, got {}",
            chunks.len()
        );

        // Each chunk should be in roughly target+overlap range (could be a bit over if a
        // sentence doesn't perfectly fit)
        for (i, c) in chunks.iter().enumerate() {
            let len = c.chars().count();
            assert!(len <= 600, "chunk {} too long: {} chars", i, len);
        }

        // Overlap check: last `overlap` chars of chunk N should be a prefix of chunk N+1
        // (modulo: chunk N+1 starts with tail-of-N then appends new content)
        for i in 0..chunks.len() - 1 {
            let prev_tail: String = chunks[i]
                .chars()
                .rev()
                .take(50)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            assert!(
                chunks[i + 1].starts_with(&prev_tail),
                "chunk {} should start with last 50 chars of chunk {}\n  expected prefix: {:?}\n  got chunk: {:?}",
                i + 1,
                i,
                prev_tail,
                &chunks[i + 1].chars().take(60).collect::<String>()
            );
        }
    }

    #[test]
    fn sentence_longer_than_target_becomes_own_chunk() {
        // A single sentence with no terminator, longer than target
        let long_sentence: String = "a".repeat(1000);
        let chunks = chunk(&long_sentence, 500, 50);

        // Should yield one chunk containing the whole sentence (we don't force-split
        // within sentences in MVP)
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].chars().count(), 1000);
    }

    #[test]
    fn mixed_cn_en_punctuation() {
        let text = "第一句中文。Second english sentence! 第三句又是中文？Fourth one.";
        let sentences = split_sentences(text);
        assert_eq!(
            sentences.len(),
            4,
            "expected 4 sentences from mixed text, got {sentences:?}"
        );
        assert!(sentences[0].contains("第一句"));
        assert!(sentences[1].contains("Second"));
        assert!(sentences[2].contains("第三句"));
        assert!(sentences[3].contains("Fourth"));
    }
}
