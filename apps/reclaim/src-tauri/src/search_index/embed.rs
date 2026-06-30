//! Embedding helpers — reuse the existing Ollama embedder (`all-minilm` via the
//! old `/api/embeddings` endpoint, already implemented in `ai::`). v1 stores f32
//! little-endian vectors as a BLOB and does cosine in Rust (sqlite-vec isn't in
//! this build). If Ollama is down, embedding returns None and the vector ranker
//! is simply skipped — FTS5 + position still rank the results.

/// Embed a piece of text. None when Ollama is unavailable or the text is empty.
pub async fn embed_text(text: &str) -> Option<Vec<f32>> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let client = crate::ai::OllamaClient::new();
    // Bound the input — embedders truncate anyway and long bodies are slow.
    let excerpt: String = text.split_whitespace().take(512).collect::<Vec<_>>().join(" ");
    client
        .generate_embedding(&excerpt, crate::ai::DEFAULT_EMBED_MODEL)
        .await
        .ok()
        .filter(|v| !v.is_empty())
}

/// f32 vector → little-endian BLOB for storage in `page_embeddings.vec`.
pub fn to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Little-endian BLOB → f32 vector. Trailing bytes that don't form a full f32 are
/// ignored (defensive against a truncated/corrupt row).
pub fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Cosine similarity in [-1, 1]. 0.0 for mismatched/zero vectors (safe default).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
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
    fn blob_roundtrips() {
        let v = vec![0.0f32, 1.5, -2.25, 1234.5];
        assert_eq!(from_blob(&to_blob(&v)), v);
    }

    #[test]
    fn from_blob_ignores_trailing_bytes() {
        let mut b = to_blob(&[1.0, 2.0]);
        b.push(0xAB); // stray byte
        assert_eq!(from_blob(&b), vec![1.0, 2.0]);
    }

    #[test]
    fn cosine_values() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 1.0], &[2.0, 2.0]) - 1.0).abs() < 1e-6);
        // mismatched length / zero vector → 0.0, never NaN/panic
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }
}
