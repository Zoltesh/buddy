# Buddy

Self-hosted AI chat application. Rust (Axum) backend + Svelte/Tailwind frontend.

## Project Structure

```
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
```

## Commands

All commands are run from the project root.

| Command | What it does |
|---------|-------------|
| `make build` | Build frontend and server (release) |
| `make build-frontend` | `cd frontend && npm install && npm run build` |
| `make build-server` | `cargo build --release` |
| `make dev` | Build frontend, then run server in debug mode (`cargo run`) |
| `make run` | Full release build, then run the server |
| `make clean` | `cargo clean` + remove `frontend/dist` and `frontend/node_modules` |
| `cargo test` | Run Rust tests |
| `cd frontend && npm run dev` | Vite dev server for frontend only |

## Code Standards

- **Minimal changes only.** Touch only the code necessary to complete the task. Do not refactor, reorganize, or "improve" surrounding code.
- **Do not introduce bugs.** Read and understand existing code before modifying it. Verify that changes preserve existing behavior unless the task explicitly requires changing it.
- **Find root causes.** When fixing a bug, diagnose the actual root cause. Do not apply workarounds, band-aids, or temporary fixes.
- **Keep changes simple.** Every change should be as small and focused as possible. Prefer the solution that impacts the least code.
- **Senior developer standards.** Write production-quality code. No TODO hacks, no commented-out code, no speculative additions. If it ships, it should be correct.
- **No collateral damage.** Do not modify function signatures, data structures, or public APIs unless the task requires it. Do not rename, reformat, or rearrange code outside the scope of the task.

## Testing

### Running tests

Run `cargo test` from the project root. All tests must pass before completing any task.

### Test organization

- Tests live **inline** in each source file inside `#[cfg(test)] mod tests { }` blocks.
- Exception: large directory modules (e.g. `api/`) keep tests in a dedicated `tests.rs` submodule declared with `#[cfg(test)] mod tests;` in `mod.rs`.
- Shared mock types and helpers live in `buddy-server/src/testutil.rs` (gated behind `#[cfg(test)]`).
- Use `#[tokio::test]` for async tests, `#[test]` for sync tests.
- Tests requiring network access are marked `#[ignore]` and run separately with `cargo test -- --ignored`.

### Shared test utilities (`testutil.rs`)

| Type | Purpose |
|------|---------|
| `MockProvider` | Returns a configurable list of text tokens |
| `SequencedProvider` | Returns a queue of `MockResponse` values (text or tool calls) |
| `MockEchoSkill` | Echoes input back; used for tool-loop tests |
| `FailingSkill` | Always returns `SkillError::ExecutionFailed` |
| `MockNoOpSkill` | Returns `{ "ok": true }` |
| `MockEmbedder` | Deterministic embedder returning rotating unit vectors |
| `parse_sse_events()` | Parses an SSE response body into `Vec<ChatEvent>` |
| `make_chat_body()` | Builds a minimal valid chat request JSON string |
| `make_chat_body_with_conversation()` | Same as above, with a conversation ID |
| `post_chat()` / `post_chat_raw()` | Sends a POST to `/api/chat` via `tower::oneshot` and returns parsed events |

### Test patterns

- **App factory functions** — Each test module defines helpers like `test_app()` and `sequenced_app()` that wire up a `Router` with mock state. Use these instead of constructing state by hand.
- **In-memory database** — Use `Store::open_in_memory()` for tests. Never touch the filesystem for database state.
- **HTTP integration via tower** — Test endpoints by sending requests through `tower::ServiceExt::oneshot()`, not by starting a real server.
- **Temp files with cleanup** — When a test needs the filesystem, create files under `std::env::temp_dir()` and clean up with `std::fs::remove_dir_all()`.
- **Error variant matching** — Use `match` or `assert!(matches!(...))` to verify specific error enum variants.

## Tech Stack

- **Backend:** Rust 2024 edition, Axum 0.8, Tokio
- **Frontend:** Svelte 5, Tailwind CSS 4, Vite 7
- **Server binds to `127.0.0.1` only** — not `0.0.0.0`
