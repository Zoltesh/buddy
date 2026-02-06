# 020 — Embedding Trait and Local Embedding Provider

## Description

Define an `Embedder` trait that abstracts text-to-vector embedding, and implement a local embedding provider using the `fastembed` crate with the `all-MiniLM-L6-v2` model. This provides zero-dependency embedding out of the box.

## Goal

buddy can embed text into vectors locally without any external API calls. The embedding abstraction supports future remote embedding providers.

## Requirements

- Define an `Embedder` trait in a new `embedding/` module:
  ```rust
  pub trait Embedder: Send + Sync {
      fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;
      fn dimensions(&self) -> usize;
      fn model_name(&self) -> &str;
  }
  ```
  - `embed` takes a batch of texts and returns a vector of embeddings (batching is more efficient for fastembed)
  - `dimensions` returns the dimensionality of the output vectors
  - `model_name` returns the model identifier string
- Define `EmbedError` enum: `ModelLoad(String)`, `EncodingFailed(String)`
- Implement `LocalEmbedder` using the `fastembed` crate:
  - Uses `all-MiniLM-L6-v2` (384 dimensions) by default
  - Model is downloaded/cached on first use by fastembed (automatic)
  - Thread-safe: fastembed's `TextEmbedding` can be shared across threads
- Add `fastembed` to `Cargo.toml` dependencies
- Construct the `LocalEmbedder` at startup when `[models.embedding]` contains a `{ type = "local" }` provider
- If `[models.embedding]` is not configured, no embedder is created (embedding features are unavailable)
- Wrap the embedder in an `Option<Arc<dyn Embedder>>` in `AppState` so downstream code can check availability

## Acceptance Criteria

- [x] `Embedder` trait compiles and is object-safe (`Arc<dyn Embedder>` works)
- [x] `LocalEmbedder` embeds text into 384-dimensional vectors using `all-MiniLM-L6-v2`
- [x] Batch embedding works (multiple texts in one call)
- [x] `dimensions()` returns `384` for the default model
- [x] `model_name()` returns `"all-MiniLM-L6-v2"`
- [x] When `[models.embedding]` is not configured, `AppState.embedder` is `None`
- [x] Embedding errors produce clear `EmbedError` messages

## Test Cases

- Embed a single text string; assert the result is a `Vec<f32>` of length 384
- Embed a batch of 3 texts; assert 3 vectors are returned, each of length 384
- Embed two semantically similar texts (e.g. "happy dog" and "joyful puppy"); assert cosine similarity is high (> 0.7)
- Embed two semantically different texts (e.g. "happy dog" and "quantum physics"); assert cosine similarity is low (< 0.5)
- Call `dimensions()` on `LocalEmbedder`; assert it returns 384
- Call `model_name()` on `LocalEmbedder`; assert it returns `"all-MiniLM-L6-v2"`

## Notes

- The `fastembed` crate handles model downloading and caching automatically. The first embedding call may take a few seconds to download the model.
- The `Embedder` trait takes `&[&str]` for batch efficiency — fastembed is significantly faster when batching.
