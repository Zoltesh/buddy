# 042 — Default Local Embedder Activation

## Description

buddy ships with a local embedder (fastembed, all-MiniLM-L6-v2, 384 dimensions) compiled into the binary. Currently, the local embedder only activates when an embedding provider is explicitly configured in `buddy.toml` under `[models.embedding]`. Users who skip that config section see a warning that memory features are disabled — even though the capability is already built in. This task makes the local embedder the automatic default so memory works out of the box with zero configuration.

## Goal

A fresh install with no `[models.embedding]` section in `buddy.toml` starts with the local embedder active and all memory features (remember, recall, automatic context retrieval) working immediately.

## Requirements

- Modify `reload::build_embedder()`: when `config.models.embedding` has no providers configured, construct and return a `LocalEmbedder` instead of `None`
- Modify `reload::build_vector_store()`: when an embedder is available (which is now always), construct the `SqliteVectorStore` as usual
- The embedder field in `AppState` (`ArcSwap<Option<Arc<dyn Embedder>>>`) continues to use `Option`, but the value is now always `Some(...)` after startup
- Log at startup which embedder is active:
  - No external configured: `"Using built-in local embedder (all-MiniLM-L6-v2, 384 dims)"`
  - External configured: `"Using external embedder: {model_name}"`
- When an external embedding provider IS configured in `[models.embedding]`, behavior is unchanged — the external provider is used, not the local one
- The `no_embedding_provider` warning (code: `no_embedding_provider`) must NOT be emitted when the local embedder is active as the default
- Do not change the `Config` struct, TOML parsing, or config schema — the local embedder is an application default, not a config value
- Do not change the `Embedder` trait or `LocalEmbedder` implementation
- Hot-reload via `reload::reload_from_config()` must also apply this default logic: if the user removes all external embedding providers, the local embedder activates

## Acceptance Criteria

- [x] `build_embedder()` returns `Some(LocalEmbedder)` when no external embedding providers are configured
- [x] `build_embedder()` returns the external embedder when `[models.embedding]` providers are configured
- [x] `build_vector_store()` creates a `SqliteVectorStore` when the embedder is available (always now)
- [x] The `no_embedding_provider` warning is not emitted on startup when the local embedder is the default
- [x] Startup log message indicates which embedder is active
- [x] Hot-reload after removing all external embedding providers activates the local embedder
- [x] All existing tests pass without modification

## Test Cases

- [x] Start with a config that has no `[models.embedding]` section; call `build_embedder()`; assert it returns `Some` containing a `LocalEmbedder` with model name `"all-MiniLM-L6-v2"` and 384 dimensions
- [x] Start with a config that has one external embedding provider; call `build_embedder()`; assert it returns the external provider, not the local embedder
- [x] Start with a config that has no `[models.embedding]` section; call `build_vector_store()` with the default embedder; assert it returns a functioning `SqliteVectorStore`
- [x] Start with a config that has no `[models.embedding]` section; collect warnings; assert the list does NOT contain a warning with code `no_embedding_provider`
- [x] Start with a config that has an external provider; call `reload_from_config()` with a config that removes all external providers; assert the embedder is now a `LocalEmbedder`
