# 019 — Provider Fallback Chain

## Description

Implement fallback logic for model slots: when the primary provider for a slot is unreachable (connection error, timeout, rate limit), buddy automatically tries the next provider in the slot's ordered list.

## Goal

Chat requests transparently fall back to secondary providers when the primary is unavailable, with no user intervention required. The user sees a brief indicator that a fallback is in use.

## Requirements

- Introduce a `ProviderChain` (or similar) that wraps an ordered `Vec<AnyProvider>` for the chat slot
- `ProviderChain` implements the same `complete` interface as a single provider
- On `complete`, try providers in order:
  - If the current provider returns `ProviderError::Network`, `ProviderError::RateLimit`, or a connection timeout, try the next provider
  - If the current provider returns `ProviderError::Auth` or `ProviderError::MalformedResponse`, do NOT fall back — these are configuration errors, not transient failures
  - If all providers fail, return the last error
- The fallback attempt is logged (to stdout/stderr for now — no formal logging framework yet)
- Add a new `ChatEvent` variant (e.g. `ChatEvent::Warning { message: String }`) to notify the frontend when a fallback occurs
  - Example message: `"Primary model unavailable, using fallback: gpt-4o"`
- The tool-call loop in `api.rs` uses `ProviderChain` instead of a single `AnyProvider`
- If the chat slot has only one provider, `ProviderChain` behaves identically to using that provider directly (no overhead)
- Build the `ProviderChain` from `config.models.chat.providers` at startup in `main.rs`

## Acceptance Criteria

- [x] When the primary provider returns a network error, the next provider in the chain is tried
- [x] When the primary provider returns a rate-limit error, the next provider is tried
- [x] Auth errors are NOT retried with fallback providers
- [x] When all providers fail, the last error is returned
- [x] A `ChatEvent::Warning` is emitted when falling back to a non-primary provider
- [x] Single-provider chains work identically to a bare provider
- [x] The tool-call loop works correctly with `ProviderChain`
- [x] Existing tests pass with `ProviderChain` wrapping a single provider

## Test Cases

- [x] Create a chain with two mock providers; first returns `Network` error, second returns text; assert the second provider is called and text is returned
- [x] Create a chain with two mock providers; first returns `RateLimit` error; assert fallback to second
- [x] Create a chain with two mock providers; first returns `Auth` error; assert fallback is NOT attempted and `Auth` error is returned
- [x] Create a chain with three mock providers; first two return `Network` errors, third succeeds; assert third provider's response is returned
- [x] Create a chain with two mock providers; both return `Network` errors; assert the last error is returned
- [x] Create a chain with one provider that succeeds; assert it works identically to calling the provider directly
- [x] Assert that when fallback occurs, the SSE stream includes a `Warning` event before the response
