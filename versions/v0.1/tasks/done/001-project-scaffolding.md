# 001 — Project Scaffolding

## Description

Initialize the Rust workspace and Svelte frontend project with the build tooling needed to develop, build, and run buddy locally.

## Goal

A developer can clone the repo, run a single command to build, and get a running (empty) web server serving a blank Svelte page.

## Requirements

- Rust 2024 edition workspace with a single binary crate (`buddy-server`)
- Svelte + Tailwind frontend project under `frontend/`
- The Rust server serves the built frontend assets as static files
- Server binds to `127.0.0.1:3000` by default
- A top-level `justfile` or `Makefile` with `dev`, `build`, and `run` targets
- `.gitignore` covering Rust, Node, and OS artifacts

## Acceptance Criteria

- [x] `cargo build` succeeds with no warnings
- [x] `npm install && npm run build` in `frontend/` produces a `dist/` directory
- [x] Running the binary serves the frontend at `http://127.0.0.1:3000`
- [x] The server does NOT bind to `0.0.0.0`
- [x] Repository has a clean `.gitignore` — no build artifacts committed

## Test Cases

- Start the server, `curl http://127.0.0.1:3000` returns HTML
- `curl http://0.0.0.0:3000` from another machine on the network fails (not bound)
