# Embedding Provider Design

## Context

buddy has a local embedder (fastembed, all-MiniLM-L6-v2, 384 dimensions) compiled into the binary. It requires no API keys, no network access, and no user configuration. Memory features (remember, recall, automatic context retrieval) depend on an active embedder.

Prior to this design, the local embedder only activated when explicitly configured in `buddy.toml`. Users who didn't add a `[models.embedding]` section saw a warning that memory features were disabled — even though the capability was already built in.

## Design Principles

1. **Memory works out of the box.** A fresh install with no embedding config uses the local embedder. No warnings, no setup required.
2. **User-owned experience.** Switching embedders is an explicit, guided action. The system never silently changes embedding models or mixes vectors from different models.
3. **Honest about state.** When something is wrong (external provider down, re-embedding needed), the UI tells the user exactly what happened and what their options are. No silent degradation.
4. **No magic fallback.** If an external embedder fails, the system does not automatically fall back to the local embedder. Different models produce incompatible vector spaces — silently switching would corrupt search results.

## Architecture

### Built-in Local Embedder (always available)

The local embedder is not a "provider" in the configuration sense. It is a built-in capability of the application:

- Always compiled in via the `fastembed` crate (ONNX runtime)
- Model: all-MiniLM-L6-v2, 384 dimensions
- Active by default when no external embedding provider is configured
- Shown in settings as a greyed-out, non-removable card labeled "Built-in"
- Cannot be turned off on its own — only overridden by adding an external provider

### External Embedding Providers

Users can add external embedding providers (e.g., OpenAI embeddings API) to override the local embedder. When an external provider is configured and healthy:

- The external provider handles all embedding operations
- The local embedder is shown as "Standby" in settings
- Existing memories may need re-embedding if the model/dimensions differ

### Provider Lifecycle

```
Fresh install          -> Local embedder active, memory works immediately
User adds external     -> "Re-embedding required. Apply now?" prompt
External goes down     -> Warning: "External embedder not responding"
User switches back     -> "Re-embedding required. Apply now?" prompt
User changes provider  -> Same re-embed prompt regardless of direction
```

## User-Facing Flows

### 1. External Embedder Goes Down

**Warning banner (chat view):**
> External embedder not responding. Memory features are unavailable until resolved.
> [Go to Settings]

**Settings view (embedding section):**
The external provider card shows an error status. Help text appears:
> Your configured embedder is not responding. To restore memory features, either:
> - Fix the external provider connection, or
> - Remove it to switch back to the built-in local embedder (re-embedding required)

### 2. Switching Embedders (any direction)

When the active embedder changes (local -> external, external -> local, external A -> external B), and stored memories exist:

**Confirmation dialog:**
> Re-embedding required. The new embedding model produces vectors in a different format than the stored memories. All memories need to be re-embedded to work with the new model.
>
> [Re-embed Now] [Cancel]

This re-embedding uses the existing `POST /api/memory/migrate` endpoint, which:
1. Loads all stored entries (source text is preserved alongside vectors)
2. Re-embeds all text with the new model
3. Replaces all vectors atomically

### 3. Dimension Changes

If a future external provider supports configurable dimensions, the same re-embed flow applies. Help text explains:

> Changing embedding dimensions requires re-embedding all stored memories. Lower dimensions are faster but less precise. Higher dimensions capture more nuance but use more storage and compute.

This is the same confirmation dialog as switching providers — the trigger is any change to the active embedding model or its configuration.

### 4. Blocked State

While migration is pending (model changed but re-embedding not yet done):
- Memory search returns `MigrationRequired` error (already implemented)
- The UI shows a persistent warning that memory is temporarily unavailable
- The settings page highlights the re-embed action

## What This Design Does NOT Include

These are deferred to future work if needed:

- **Automatic fallback chain for embedders.** Unlike chat providers, embedders cannot gracefully fall back because vectors from different models are incompatible.
- **Dimension conversion (truncation/padding).** Vectors from different models occupy different semantic spaces. Converting dimensions between models produces meaningless results. Full re-embedding is the only correct approach.
- **Background re-embedding.** Migration is synchronous and user-triggered. For large memory stores, async migration with progress reporting could be added later.
- **Multiple concurrent embedders.** Only one embedder is active at a time. A "chain" pattern (try external, fall back to local) is not implemented because it would mix incompatible vectors.

## Relationship to Existing Infrastructure

| Component | Status | Used By This Design |
|-----------|--------|-------------------|
| `LocalEmbedder` (fastembed) | Implemented | Yes — becomes the always-on default |
| `VectorStore::needs_migration()` | Implemented | Yes — detects model/dimension mismatch |
| `POST /api/memory/migrate` | Implemented | Yes — re-embeds all entries |
| Source text preservation | Implemented | Yes — enables re-embedding from scratch |
| `embedding_dimension_mismatch` warning | Implemented | Yes — surfaces mismatch to UI |
| Warning banner with Settings link | Implemented | Yes — guides user to resolve issues |
| Embedder health check endpoint | Not implemented | Needed for "external down" detection |
| Re-embed confirmation dialog | Not implemented | Needed for switch/migration flow |
| Built-in embedder card in settings | Not implemented | Needed for visibility |
