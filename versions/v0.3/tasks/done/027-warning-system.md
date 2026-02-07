# 027 — Warning System

## Description

Implement a structured warning system that surfaces non-fatal configuration or runtime issues to the frontend. Warnings are informational — they do not block functionality but tell the user when features are degraded.

## Goal

Users are clearly informed when buddy is operating in a degraded mode (e.g. no embedding model, fallback provider active) without intrusive errors blocking their workflow.

## Requirements

- Define a `Warning` struct:
  ```rust
  pub struct Warning {
      pub code: String,        // machine-readable identifier (e.g. "no_embedding_model")
      pub message: String,     // human-readable description
      pub severity: WarningSeverity,
  }
  ```
  - `WarningSeverity` enum: `Info`, `Warning` (two levels are sufficient for now)
- Implement a `WarningCollector` that accumulates warnings during startup and runtime:
  - `add(warning)` — add a warning
  - `list() -> &[Warning]` — get all current warnings
  - `clear(code)` — remove warnings with a given code (e.g. when the issue is resolved)
- Add a `GET /api/warnings` endpoint that returns current warnings as JSON
- Emit warnings during startup for known degraded states:
  - `no_embedding_model`: `[models.embedding]` is not configured — memory features are disabled
  - `no_vector_store`: Vector store failed to initialize — long-term memory unavailable
  - `single_chat_provider`: Only one chat provider configured — no fallback available
  - `embedding_dimension_mismatch`: Stored embeddings don't match current model (from task 022)
- The frontend can poll or receive warnings to display a non-intrusive banner
- Warnings are included in the SSE stream at the start of each chat response (as a `ChatEvent::Warnings` event) so the frontend doesn't need a separate polling mechanism
- Store `WarningCollector` in `AppState` (wrapped in `Arc<RwLock<>>` for runtime updates)

## Acceptance Criteria

- [x] `Warning` struct and `WarningCollector` are implemented
- [x] `GET /api/warnings` returns current warnings as JSON
- [x] Startup without `[models.embedding]` produces a `no_embedding_model` warning
- [x] Startup with one chat provider produces a `single_chat_provider` info
- [x] Warnings are included as a `ChatEvent::Warnings` event in SSE chat streams
- [x] Warnings can be cleared when the underlying issue is resolved
- [x] Warning messages are human-readable and actionable (tell the user what to do)

## Test Cases

- [x] Start with no `[models.embedding]` config; call `GET /api/warnings`; assert `no_embedding_model` warning is present
- [x] Start with a valid full config; call `GET /api/warnings`; assert no warnings (or only informational)
- [x] Start with one chat provider; assert `single_chat_provider` info is present
- [x] Add a warning at runtime; call `GET /api/warnings`; assert it appears
- [x] Clear a warning by code; call `GET /api/warnings`; assert it is removed
- [x] Send a chat request; assert the SSE stream begins with a `Warnings` event (if warnings exist)
- [x] Assert warning messages include guidance (e.g. "edit buddy.toml under [models.embedding]")
