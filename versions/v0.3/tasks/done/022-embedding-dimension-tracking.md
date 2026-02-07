# 022 — Embedding Dimension Tracking and Migration

## Description

Implement detection and handling of embedding model changes. When a user switches to an embedding model with different dimensions, the vector store detects the mismatch and prompts the user to re-embed all stored memories using the new model.

## Goal

Switching embedding models never silently corrupts memory. The system detects mismatches, blocks mixed-dimension queries, and re-embeds from stored source text when the user approves.

## Requirements

- On startup, compare the configured embedder's `dimensions()` and `model_name()` against the vector store's `metadata()`
- If the store is empty, record the current model's dimensions and name as the store baseline
- If the store has entries and the model name or dimensions differ:
  - Emit a warning (via the warning system, task 027, or a log message if 027 isn't done yet)
  - Block all memory search operations until migration is resolved
  - Expose a migration status via an API endpoint or internal state
- Implement a re-embedding migration:
  - Read all entries from the vector store (source text is always available)
  - Re-embed each entry's source text using the new model
  - Replace the old embeddings with new ones
  - Update the store's model metadata
- The migration can be triggered via an API endpoint (e.g. `POST /api/memory/migrate`)
- During migration, memory features are unavailable (searches return empty or an error)
- Provide a discard option: `DELETE /api/memory` clears all stored memories instead of migrating
- Never mix embeddings from different models/dimensions in the same store

## Acceptance Criteria

- [x] Startup detects when the configured embedding model differs from stored vectors
- [x] Memory searches are blocked when a dimension mismatch is detected
- [x] `POST /api/memory/migrate` re-embeds all entries using the new model
- [x] After migration, store metadata reflects the new model name and dimensions
- [x] `DELETE /api/memory` clears all entries and resets store metadata
- [x] An empty store adopts the current model's metadata on first write
- [x] No mixing of embeddings from different models ever occurs

## Test Cases

- [x] Store entries with model "A" (dim 384); switch to model "B" (dim 768); assert mismatch is detected on startup
- [x] With a mismatch detected, attempt a memory search; assert it is blocked (returns error or empty)
- [x] Trigger migration with 3 stored entries; assert all 3 are re-embedded with the new model's dimensions
- [x] After migration, call `metadata()`; assert model name and dimensions match the new model
- [x] Trigger `DELETE /api/memory`; assert store is empty and metadata is reset
- [x] Start with an empty store and model "A"; store one entry; assert metadata shows model "A" with correct dimensions
- [x] Store entries, switch models, migrate; then search; assert results use the new embeddings

## Notes

- For V0.3 scale (hundreds to low thousands of entries), re-embedding the entire store is fast enough to do synchronously. If this becomes a bottleneck, async migration with progress tracking can be added later.
- The migration endpoint is intentionally explicit (user-triggered) rather than automatic to avoid surprise compute costs or delays.
