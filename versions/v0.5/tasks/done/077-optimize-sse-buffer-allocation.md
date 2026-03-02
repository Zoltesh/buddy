# 077 — Optimize SSE Stream Buffer Allocation

## Description

All provider streaming implementations (`openai.rs`, `gemini.rs`, and likely `lmstudio.rs`, `ollama.rs`, `mistral.rs`) use `buffer = buffer[pos + 1..].to_string()` inside their SSE parsing loops. This allocates a new `String` on every newline character encountered in the stream. While functionally correct, this creates unnecessary allocations during streaming — one per SSE line, which can be hundreds per response.

## Goal

Replace per-line `String` allocation with an in-place drain approach in all SSE stream parsers.

## Requirements

- In each provider's `parse_*_stream()` function, find the pattern:
  ```rust
  buffer = buffer[pos + 1..].to_string();
  ```
- Replace it with `drain()` which modifies the string in-place without allocation:
  ```rust
  buffer.drain(..pos + 1);
  ```
- `String::drain()` removes the bytes from the front of the string and shifts the remaining bytes down. No new allocation occurs.
- Apply this change to every provider that has this pattern. Check all files in `buddy-core/src/provider/`:
  - `openai.rs` — confirmed at line ~333
  - `gemini.rs` — confirmed at line ~261
  - `lmstudio.rs` — check for same pattern
  - `ollama.rs` — check for same pattern
  - `mistral.rs` — check for same pattern
- Do NOT change any other logic in the parsing functions. Only replace the buffer slicing line.

## Files to Modify

- `buddy-core/src/provider/openai.rs` — replace buffer slice with drain
- `buddy-core/src/provider/gemini.rs` — replace buffer slice with drain
- `buddy-core/src/provider/lmstudio.rs` — replace if pattern exists
- `buddy-core/src/provider/ollama.rs` — replace if pattern exists
- `buddy-core/src/provider/mistral.rs` — replace if pattern exists

## Acceptance Criteria

- [x] No provider file contains `buffer[pos + 1..].to_string()` or similar slicing pattern
- [x] All providers use `buffer.drain(..pos + 1)` (or equivalent zero-alloc approach)
- [x] Streaming behavior is unchanged — same SSE events, same order, same content
- [x] All existing tests pass (`cargo test`)

## Test Cases

- [x] Run existing SSE parsing tests for each provider; assert they produce identical output (regression check)
- [x] Run `cargo test`; assert all tests pass
