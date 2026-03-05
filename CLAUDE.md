# CLAUDE.md — Buddy Project Contract (canonical)

## Prime Directive
- Do not guess. If something is unknown, find it in the repo or create a task to discover it.
- Never claim a command passed unless you ran the exact command and it succeeded.
- Prefer small, testable changes. No “big bang” edits.
- Follow the workflow contract: implementation work happens via task cards in `tasks/`.

## Project Summary
Buddy is a self-hosted AI chat application with:
- Backend: Rust (Axum) server in `buddy-server/`
- Frontend: Svelte + Tailwind SPA in `frontend/`
- Work tracking: file-based kanban board in `tasks/`

## Project Structure
    buddy-server/       Rust binary crate (Axum web server)
      src/
        api/            HTTP API module (mod.rs + chat, config, conversation, memory, tests)
        skill/          Skill implementations (remember, recall, read_file, etc.)
        provider/       LLM provider adapters (openai, lmstudio)
    frontend/           Svelte + Tailwind SPA (built with Vite)
      src/lib/
        settings/       Settings tab components (GeneralTab, ModelsTab, SkillsTab)
    tasks/
      backlog/      Tasks not yet started
      in-progress/  Tasks currently being worked on
      blocked/      Tasks that cannot proceed
      done/         Completed tasks
    features/
      01-ideas/
      02-refining/
      03-ready/
      04-archived/

Staleness policy for this section:
- This structure is an orientation map, not a full inventory.
- Update it only when a task introduces a meaningful structural change (new top-level folder, major module move).

## Commands (source of truth)
All commands are run from the project root unless noted. Do not invent commands.

### Build / Run
- Full release build: `make build`
- Frontend build (installs deps + builds): `make build-frontend`
- Server release build: `make build-server`
- Dev mode: `make dev`
- Full release build then run: `make run`
- Clean: `make clean`
- Frontend dev server (frontend only): `cd frontend && npm run dev`

### Formatting
- Backend format: `cargo fmt`
- Frontend format (must exist as a script): `cd frontend && npm run format`

If the frontend format script does not exist, do not invent it. Create a task to add it.

### Linting
- Backend lint: `cargo clippy --all-targets --all-features -- -D warnings`

If clippy is not installed, install once via: `rustup component add clippy`

- Frontend lint: none defined. Do not invent one.
- Frontend format check (acts as lint gate): `cd frontend && npm run format:check`

### Tests
- Rust tests: `cargo test`
- Ignored tests: `cargo test -- --ignored`

## Engineering Standards (hard rules)
- Minimal changes only. Touch only the code necessary to complete the task.
- Do not refactor, reorganize, or “improve” surrounding code unless the task explicitly requires it.
- Find the root cause; do not apply band-aids.
- No commented-out code, no speculative additions, no TODO hacks.
- No collateral damage: do not change public APIs, signatures, or shared data structures unless the task requires it.

## Testing Contract (hard rules)
Before moving any task to Done:
- Always run: `cargo test`
- If you changed Rust code, also run: `cargo clippy --all-targets --all-features -- -D warnings`
- If you changed frontend code, also run:
  - `cd frontend && npm run format:check`
  - `make build-frontend`

Do not check off validations unless you ran the command and it succeeded.

### Test organization (Rust)
- Tests live inline in each source file inside `#[cfg(test)] mod tests { }` blocks.
- Exception: large directory modules (e.g. `api/`) keep tests in a dedicated `tests.rs` submodule declared with `#[cfg(test)] mod tests;` in `mod.rs`.
- Shared mock types and helpers live in `buddy-server/src/testutil.rs` (gated behind `#[cfg(test)]`).
- Use `#[tokio::test]` for async tests, `#[test]` for sync tests.
- Tests requiring network access are marked `#[ignore]` and run separately with `cargo test -- --ignored`.

Testutil staleness policy:
- Do not list individual helpers here. Treat `buddy-server/src/testutil.rs` as the inventory.
- When you need a mock/helper, read that file and use what exists.

### Test patterns (Rust)
- App factory helpers: use module helpers like `test_app()` / `sequenced_app()` rather than constructing app state by hand.
- In-memory database: use `Store::open_in_memory()` for tests. Never persist DB state to disk in tests unless the task explicitly requires it.
- HTTP integration: use `tower::ServiceExt::oneshot()` (do not start a real server).
- Temp files: use `std::env::temp_dir()` and clean up with `std::fs::remove_dir_all()`.
- Error variants: use `match` or `assert!(matches!(...))` to verify specific enum variants.

## Network / Binding
- The server binds to `127.0.0.1` only (not `0.0.0.0`) unless a task explicitly changes it.

## Workflow Contract

### Tasks (execution)
- Implementation work happens via task cards under `tasks/`.
- A task is Done only when:
  1) Acceptance Criteria are checked off
  2) Validations/Test Cases are checked off
  3) Required commands were actually run

### Features (spec -> tasks)
- If the work needs TWO OR MORE task cards, it is a Feature.
- Features move through:
  - `features/01-ideas/` (unstructured idea)
  - `features/02-refining/<FEATURE_ID>/` (draft PRD + questions)
  - `features/03-ready/<FEATURE_ID>/` (locked PRD v1 + amendments)
  - `features/04-archived/<FEATURE_ID>/` (outcome notes)

### When something is bigger than expected
- If a task grows beyond a single focused change:
  - stop
  - create additional task cards
  - do not silently expand scope
- If a PRD/task is missing required details:
  - do not fill gaps by invention
  - create a discovery task or send the feature back to refining

## CLAUDE.md Maintenance Rule
- Do not edit this file during normal work unless a task explicitly requires it.
- If you discover this file is inaccurate (wrong commands, moved directories, changed policies):
  - create a task card: "Update CLAUDE.md"
  - or update it only if the current task explicitly includes keeping docs accurate.
