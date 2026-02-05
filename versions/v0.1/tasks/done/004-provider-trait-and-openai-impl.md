# 004 — Provider Trait and OpenAI-Compatible Implementation

## Description

Define the `Provider` trait that abstracts LLM interaction, and implement it for OpenAI-compatible APIs (covering OpenAI, Azure OpenAI, and any endpoint that speaks the same protocol).

## Goal

The system can send a conversation to an LLM and receive a streaming response through a clean async trait. The trait must be general enough that Anthropic, Ollama, and others can implement it in V0.2+ without changes to the trait itself.

## Requirements

- A `Provider` trait with at minimum:
  - `async fn complete(&self, messages: Vec<Message>, config: &ProviderConfig) -> Result<impl Stream<Item = Result<Token, ProviderError>>, ProviderError>`
- A `Token` type representing a chunk of streamed output (at minimum: text delta)
- A `ProviderError` enum covering: network failure, auth failure, rate limit, malformed response, and an `Other(String)` catch-all
- An `OpenAiProvider` struct implementing the trait:
  - Uses the `reqwest` client to call the chat completions endpoint with `stream: true`
  - Parses SSE `data:` lines into `Token` values
  - Handles `[DONE]` sentinel
- The provider is constructed from `Config.provider` fields — no hardcoded URLs or keys

## Acceptance Criteria

- [x] `OpenAiProvider` successfully streams a response from an OpenAI-compatible endpoint given valid credentials
- [x] Each `Token` arrives as it is generated (true streaming, not buffered)
- [x] Network errors, 401s, and 429s are mapped to the correct `ProviderError` variant
- [x] The `Provider` trait compiles as an `async trait` with no `Box<dyn>` in the stream return type if possible (or with a clear type alias if boxing is needed)
- [x] No OpenAI-specific logic leaks outside of `OpenAiProvider`

## Test Cases

- Unit: construct a `Vec<Message>`, serialize to the expected OpenAI request JSON, assert shape matches the API spec
- Unit: parse a sample SSE stream (hard-coded bytes) into a sequence of `Token` values; assert content matches
- Unit: parse an SSE stream containing an error response; assert `ProviderError` variant is correct
- Integration (requires live key): send a short prompt, collect streamed tokens, assert the joined result is non-empty valid text
