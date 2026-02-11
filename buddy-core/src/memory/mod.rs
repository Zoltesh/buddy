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
    /// The embedding model has changed; migration is required before searches.
    MigrationRequired,
}

impl fmt::Display for VectorStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageError(msg) => write!(f, "storage error: {msg}"),
            Self::DimensionMismatch { expected, got } => {
                write!(f, "dimension mismatch: expected {expected}, got {got}")
            }
            Self::NotFound(msg) => write!(f, "not found: {msg}"),
            Self::MigrationRequired => write!(
                f,
                "embedding model has changed; run migration before searching"
            ),
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

/// Information about the stored embeddings model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredModelInfo {
    pub model_name: String,
    pub dimensions: usize,
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
    /// Return all stored entries (used for re-embedding during migration).
    fn list_all(&self) -> Result<Vec<VectorEntry>, VectorStoreError>;
    /// Delete all entries and reset migration state.
    fn clear(&self) -> Result<(), VectorStoreError>;
    /// Returns true when the embedding model has changed and migration is needed.
    fn needs_migration(&self) -> bool;
    /// Returns the number of stored entries.
    fn count(&self) -> Result<usize, VectorStoreError>;
    /// Returns the model name and dimensions from the stored vectors (not the current config).
    /// Returns None if the store is empty.
    fn stored_model_info(&self) -> Result<Option<StoredModelInfo>, VectorStoreError>;
}
