# 044 — Embedding Migration Detection

## Description

When a user changes their embedding provider (e.g., switches from local to OpenAI, or changes the external model), existing stored memories become incompatible because different models produce vectors in different semantic spaces. The system must detect this mismatch and inform the frontend so it can prompt the user to re-embed. The `VectorStore::needs_migration()` method and `POST /api/memory/migrate` endpoint already exist. This task adds the detection and signaling layer: after saving embedding config changes, the backend tells the frontend whether re-embedding is needed, and a new status endpoint lets the frontend check migration state at any time.

## Goal

After saving a config change that affects the embedding model, the API response tells the frontend whether existing memories need re-embedding, and a status endpoint provides the current migration state on demand.

## Requirements

- Modify the `PUT /api/config/models` response: after successfully saving, check `vector_store.needs_migration()`. Add a field to the response:
  ```json
  {
    "embedding_migration_required": true
  }
  ```
  - `true` when the active embedder's model/dimensions differ from what is stored in the vector store AND the vector store has at least one entry
  - `false` otherwise (no mismatch, or no stored memories)
- Add `GET /api/memory/status` endpoint:
  ```json
  {
    "total_entries": 42,
    "migration_required": false,
    "stored_model": "all-MiniLM-L6-v2",
    "stored_dimensions": 384,
    "active_model": "all-MiniLM-L6-v2",
    "active_dimensions": 384
  }
  ```
  - `total_entries`: count of vectors in the store
  - `migration_required`: same logic as `needs_migration()`
  - `stored_model` / `stored_dimensions`: the model name and dimensions recorded in the vector store metadata (what the existing vectors were embedded with)
  - `active_model` / `active_dimensions`: the currently active embedder's model name and dimensions
  - If the vector store is empty, `stored_model` and `stored_dimensions` are `null`
- Add a `count()` method to the `VectorStore` trait that returns the number of stored entries. Implement it on `SqliteVectorStore` with `SELECT COUNT(*) FROM vectors`
- The existing `POST /api/memory/migrate` endpoint is unchanged — it already handles the re-embedding
- Register the new route in `main.rs`
- Do not modify the migration logic itself

## Acceptance Criteria

- [x] `PUT /api/config/models` response includes `embedding_migration_required` boolean
- [x] `embedding_migration_required` is `true` when embedder model changed and memories exist
- [x] `embedding_migration_required` is `false` when embedder model unchanged
- [x] `embedding_migration_required` is `false` when no memories exist (even if model changed)
- [x] `GET /api/memory/status` returns the specified JSON structure
- [x] `GET /api/memory/status` reflects the current state accurately after config changes
- [x] `VectorStore` trait has a `count()` method
- [x] The existing `POST /api/memory/migrate` still works unchanged

## Test Cases

- [x] Store one memory with model "A"; change config to model "B"; call `PUT /api/config/models`; assert response contains `embedding_migration_required: true`
- [x] Store zero memories; change config to a different model; call `PUT /api/config/models`; assert response contains `embedding_migration_required: false`
- [x] Save config without changing the embedding model; call `PUT /api/config/models`; assert response contains `embedding_migration_required: false`
- [x] Call `GET /api/memory/status` with an empty store; assert `total_entries: 0`, `migration_required: false`, `stored_model: null`
- [x] Store a memory with model "all-MiniLM-L6-v2", 384 dims; change active embedder to a mock with model "text-embedding-3-small", 1536 dims; call `GET /api/memory/status`; assert `migration_required: true`, correct stored/active values
- [x] Call `POST /api/memory/migrate` after a model change; then call `GET /api/memory/status`; assert `migration_required: false` and `stored_model` matches the new active model
