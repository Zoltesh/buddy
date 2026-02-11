pub mod local;

use std::fmt;

/// Errors that can occur during embedding.
#[derive(Debug)]
pub enum EmbedError {
    /// Failed to load or initialize the embedding model.
    ModelLoad(String),
    /// Failed to encode input texts into vectors.
    EncodingFailed(String),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModelLoad(msg) => write!(f, "model load error: {msg}"),
            Self::EncodingFailed(msg) => write!(f, "encoding failed: {msg}"),
        }
    }
}

impl std::error::Error for EmbedError {}

/// Trait abstracting text-to-vector embedding.
pub trait Embedder: Send + Sync {
    /// Embed a batch of texts into vectors.
    fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;

    /// Dimensionality of the output vectors.
    fn dimensions(&self) -> usize;

    /// Model identifier string.
    fn model_name(&self) -> &str;

    /// Provider type identifier (e.g., "local", "openai").
    fn provider_type(&self) -> &str;
}
