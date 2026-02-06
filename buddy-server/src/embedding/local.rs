use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

use super::{EmbedError, Embedder};

const DEFAULT_MODEL: EmbeddingModel = EmbeddingModel::AllMiniLML6V2;
const DEFAULT_DIMENSIONS: usize = 384;
const MODEL_NAME: &str = "all-MiniLM-L6-v2";

/// Local embedding provider using fastembed with ONNX runtime.
pub struct LocalEmbedder {
    model: Mutex<TextEmbedding>,
}

impl LocalEmbedder {
    pub fn new() -> Result<Self, EmbedError> {
        let options = InitOptions::new(DEFAULT_MODEL).with_show_download_progress(true);
        let model = TextEmbedding::try_new(options)
            .map_err(|e| EmbedError::ModelLoad(e.to_string()))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut model = self.model.lock().unwrap();
        model
            .embed(texts.to_vec(), None)
            .map_err(|e| EmbedError::EncodingFailed(e.to_string()))
    }

    fn dimensions(&self) -> usize {
        DEFAULT_DIMENSIONS
    }

    fn model_name(&self) -> &str {
        MODEL_NAME
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::*;

    /// Shared model instance across all tests to avoid parallel download races
    /// and redundant model loads.
    static EMBEDDER: LazyLock<LocalEmbedder> =
        LazyLock::new(|| LocalEmbedder::new().unwrap());

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (norm_a * norm_b)
    }

    #[test]
    fn embed_single_text_returns_384_dims() {
        let result = EMBEDDER.embed(&["hello world"]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 384);
    }

    #[test]
    fn embed_batch_of_three_returns_three_vectors() {
        let result = EMBEDDER.embed(&["one", "two", "three"]).unwrap();
        assert_eq!(result.len(), 3);
        for vec in &result {
            assert_eq!(vec.len(), 384);
        }
    }

    #[test]
    fn similar_texts_have_high_cosine_similarity() {
        let result = EMBEDDER
            .embed(&["the cat sat on the mat", "the cat is sitting on the mat"])
            .unwrap();
        let sim = cosine_similarity(&result[0], &result[1]);
        assert!(
            sim > 0.7,
            "expected cosine similarity > 0.7 for similar texts, got {sim}"
        );
    }

    #[test]
    fn different_texts_have_low_cosine_similarity() {
        let result = EMBEDDER
            .embed(&["happy dog", "quantum physics"])
            .unwrap();
        let sim = cosine_similarity(&result[0], &result[1]);
        assert!(
            sim < 0.5,
            "expected cosine similarity < 0.5 for different texts, got {sim}"
        );
    }

    #[test]
    fn dimensions_returns_384() {
        assert_eq!(EMBEDDER.dimensions(), 384);
    }

    #[test]
    fn model_name_returns_expected() {
        assert_eq!(EMBEDDER.model_name(), "all-MiniLM-L6-v2");
    }
}
