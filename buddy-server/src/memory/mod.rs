pub mod sqlite;

use std::fmt;

use serde::{Deserialize, Serialize};

/// Errors that can occur during vector store operations.
#[derive(Debug)]
pub enum VectorStoreError {
    /// A general storage failure (I/O, SQL, etc.).
    StorageError(String),
    /// Embedding dimension does not match the store's configured dimension.
    DimensionMismatch { expected: usize, got: usize },
    /// Requested entry was not found.
    NotFound(String),
}

impl fmt::Display for VectorStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageError(msg) => write!(f, "storage error: {msg}"),
            Self::DimensionMismatch { expected, got } => {
                write!(f, "dimension mismatch: expected {expected}, got {got}")
            }
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
        }
    }
}

impl std::error::Error for VectorStoreError {}

/// A vector entry to be stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEntry {
    pub id: String,
    pub embedding: Vec<f32>,
    pub source_text: String,
    pub metadata: serde_json::Value,
}

/// A search result with similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub source_text: String,
    pub metadata: serde_json::Value,
    pub score: f32,
}

/// Metadata about the vector store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreMetadata {
    pub model_name: String,
    pub dimensions: usize,
    pub entry_count: usize,
}

/// Trait abstracting vector storage and similarity search.
pub trait VectorStore: Send + Sync {
    fn store(&self, entry: VectorEntry) -> Result<(), VectorStoreError>;
    fn search(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, VectorStoreError>;
    fn delete(&self, id: &str) -> Result<(), VectorStoreError>;
    fn metadata(&self) -> Result<StoreMetadata, VectorStoreError>;
}
