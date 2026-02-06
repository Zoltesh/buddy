# 021 — VectorStore Trait and SQLite Implementation

## Description

Define a `VectorStore` trait that abstracts vector storage and similarity search, and implement a SQLite-backed vector store that stores embeddings alongside their source text and metadata.

## Goal

buddy has a persistent, zero-config vector store for long-term memory that supports storing and searching embeddings. The trait boundary allows alternative backends (Qdrant, ChromaDB) in the future.

## Requirements

- Define a `VectorStore` trait in a new `memory/` module:
  ```rust
  pub trait VectorStore: Send + Sync {
      fn store(&self, entry: VectorEntry) -> Result<(), VectorStoreError>;
      fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>, VectorStoreError>;
      fn delete(&self, id: &str) -> Result<(), VectorStoreError>;
      fn metadata(&self) -> Result<StoreMetadata, VectorStoreError>;
  }
  ```
- Define supporting types:
  - `VectorEntry { id: String, embedding: Vec<f32>, source_text: String, metadata: serde_json::Value }`
  - `SearchResult { id: String, source_text: String, metadata: serde_json::Value, score: f32 }`
  - `StoreMetadata { model_name: String, dimensions: usize, entry_count: usize }`
  - `VectorStoreError` enum: `StorageError(String)`, `DimensionMismatch { expected: usize, got: usize }`, `NotFound(String)`
- Implement `SqliteVectorStore`:
  - Uses the existing SQLite infrastructure (new table in the same database, or a dedicated `memory.db`)
  - Schema stores: `id`, `embedding` (as blob), `source_text`, `metadata` (JSON), `model_name`, `dimensions`, `created_at`
  - Similarity search uses cosine similarity computed in Rust (not SQL) — load candidates and rank in-memory
  - On `store`, validate that the embedding dimension matches the store's configured dimension
  - `metadata()` returns the model name, dimensions, and entry count
- Source text is **always** stored alongside embeddings — this enables lossless re-embedding when models change
- The store records which embedding model produced each vector (model name stored per-entry)
- Initialize the vector store at startup when an embedder is available; pass it as `Option<Arc<dyn VectorStore>>` in `AppState`

## Acceptance Criteria

- [x] `VectorStore` trait compiles and is object-safe (`Arc<dyn VectorStore>` works)
- [x] `SqliteVectorStore` can store and retrieve vector entries
- [x] Similarity search returns results ordered by descending cosine similarity
- [x] Storing an embedding with wrong dimensions returns `DimensionMismatch` error
- [x] `metadata()` returns correct model name, dimensions, and entry count
- [x] Source text is stored and returned in search results
- [x] The vector store table is created automatically on first use (migration)

## Test Cases

- Store an entry; search with the same embedding; assert the entry is returned with score ~1.0
- Store 5 entries with known embeddings; search with a query embedding; assert results are ordered by similarity
- Store an entry; call `delete(id)`; search again; assert the entry is no longer returned
- Attempt to store an embedding of wrong dimension (e.g. 128 into a 384-dim store); assert `DimensionMismatch` error
- Call `metadata()` after storing 3 entries; assert `entry_count` is 3 and dimensions/model_name are correct
- Search an empty store; assert empty results (not an error)
- Store an entry with JSON metadata; retrieve it; assert metadata is preserved exactly

## Notes

- Cosine similarity in Rust is straightforward: `dot(a, b) / (norm(a) * norm(b))`. For V0.3's scale (hundreds to low thousands of entries), brute-force search in memory is sufficient. Approximate nearest-neighbor indexing is a future optimization.
- Consider using a separate `memory.db` file to keep conversation data and memory data independent. This simplifies backup and migration.
