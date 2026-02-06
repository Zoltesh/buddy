# Buddy

Self-hosted AI chat application. Rust (Axum) backend + Svelte/Tailwind frontend.

## Project Structure

```
buddy-server/       Rust binary crate (Axum web server)
frontend/           Svelte + Tailwind SPA (built with Vite)
versions/           Version specs and task boards
  v0.X/
    v0.X.md         Version spec (goals, scope, architectural decisions)
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

## Task Workflow

Each version has a task board under `versions/vX.Y/tasks/` with four directories: `backlog/`, `in-progress/`, `blocked/`, `done/`.

### Picking up a task

1. Move the task file from `backlog/` to `in-progress/`.
2. Work the task until all acceptance criteria are met.
3. Check off checklist items (`- [ ]` → `- [x]`) in the task file as each one is completed.

### Running test cases

Every task includes a **Test Cases** section. These tests **must** be run and verified before a task can be considered complete.

### Blocked tasks

If a task cannot proceed (missing dependency, unresolved question, etc.), move the task file from `in-progress/` to `blocked/`.

### Completing a task

When all acceptance criteria are met and all test cases pass, move the task file from `in-progress/` to `done/`.

## Code Standards

- **Minimal changes only.** Touch only the code necessary to complete the task. Do not refactor, reorganize, or "improve" surrounding code.
- **Do not introduce bugs.** Read and understand existing code before modifying it. Verify that changes preserve existing behavior unless the task explicitly requires changing it.
- **Find root causes.** When fixing a bug, diagnose the actual root cause. Do not apply workarounds, band-aids, or temporary fixes.
- **Keep changes simple.** Every change should be as small and focused as possible. Prefer the solution that impacts the least code.
- **Senior developer standards.** Write production-quality code. No TODO hacks, no commented-out code, no speculative additions. If it ships, it should be correct.
- **No collateral damage.** Do not modify function signatures, data structures, or public APIs unless the task requires it. Do not rename, reformat, or rearrange code outside the scope of the task.

## Tech Stack

- **Backend:** Rust 2024 edition, Axum 0.8, Tokio
- **Frontend:** Svelte 5, Tailwind CSS 4, Vite 7
- **Server binds to `127.0.0.1` only** — not `0.0.0.0`
