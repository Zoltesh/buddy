# 045 — Embedding Settings UI

## Description

The ModelsTab embedding section currently shows a hardcoded "Built-in Local" card. This task updates the embedding UI to reflect the actual embedder state: whether the built-in is active or on standby, the health of the active embedder, and a re-embed confirmation dialog when switching embedding providers causes a migration. This completes the user-facing embedding experience described in `versions/v0.5/embedding_design.md`.

## Goal

Users see accurate embedding status in Settings, are prompted to re-embed when switching providers, and can trigger re-embedding from a clear confirmation dialog.

## Requirements

- **Built-in embedder card:**
  - Always shown as the first card in the embedding section (not removable, not editable)
  - When no external provider is configured: show a green "Active" badge, model name, dimensions
  - When an external provider is configured: show a gray "Standby" badge with text "Overridden by external provider"
  - Display health status from `GET /api/embedder/health` (task 043)
- **Embedder health polling:**
  - On mount and after saving embedding config changes, call `GET /api/embedder/health`
  - If `status: "unhealthy"`: show a red warning on the active provider card with the error message
  - Do not poll continuously — only check on mount and after saves
- **Re-embed confirmation dialog:**
  - After saving embedding config changes via `PUT /api/config/models`, check the `embedding_migration_required` field in the response (task 044)
  - If `true`, show a modal dialog:
    - Title: "Re-embedding Required"
    - Body: "The new embedding model produces vectors in a different format than the stored memories. All {N} memories need to be re-embedded to work with the new model."
    - Buttons: "Re-embed Now" (primary) and "Later" (secondary)
  - "Re-embed Now" calls `POST /api/memory/migrate`, shows a spinner during the request, and shows success/failure inline
  - "Later" dismisses the dialog. A persistent yellow banner remains in the embedding section: "Memories need re-embedding. [Re-embed Now]"
  - Fetch `GET /api/memory/status` (task 044) to get the entry count for the dialog body
- **No backend changes.** This is a frontend-only task. It depends on tasks 042, 043, and 044 being complete.

## Acceptance Criteria

- [x] Built-in embedder card shows "Active" badge when no external provider is configured
- [x] Built-in embedder card shows "Standby" badge when an external provider is configured
- [x] Embedder health is fetched on mount and displayed on the active provider card
- [x] Unhealthy embedder shows red warning with error message
- [x] Saving embedding config that requires migration shows the re-embed confirmation dialog
- [x] Dialog shows the correct number of memories that need re-embedding
- [x] "Re-embed Now" calls `POST /api/memory/migrate` and shows success/failure
- [x] "Later" dismisses the dialog and shows a persistent warning banner
- [x] Saving embedding config that does NOT require migration shows no dialog
- [x] All existing embedding section functionality works (add, edit, delete, reorder external providers)

## Test Cases

- [x] Load Settings > Models with no external embedding provider; built-in card shows green "Active" badge, model "all-MiniLM-L6-v2", dimensions "384"
- [x] Add an external embedding provider and save; built-in card changes to gray "Standby" badge with "Overridden by external provider"
- [x] Remove the external provider and save; built-in card returns to green "Active"
- [x] Configure a mock external embedder that fails health check; load Settings > Models; active card shows red warning with error text
- [x] Add an external provider when memories exist; save; re-embed dialog appears with "Re-embedding Required" title and correct memory count
- [x] In the re-embed dialog, click "Re-embed Now"; spinner appears; on success, dialog closes and section returns to normal
- [x] In the re-embed dialog, click "Later"; dialog closes; yellow banner appears in embedding section with "Re-embed Now" link
- [x] Click "Re-embed Now" in the yellow banner; re-embedding runs; banner disappears on success
- [x] Save embedding config without changing the model; no re-embed dialog appears
- [x] Add, edit, delete, and reorder external embedding providers; all existing functionality still works
