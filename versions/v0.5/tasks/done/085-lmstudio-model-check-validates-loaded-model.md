# 085 — LM Studio Model Check Validates Loaded Model

## Description

When testing an LM Studio provider connection, the current implementation sends a chat completion request to `/chat/completions` with the configured model name. However, LM Studio does not fail the request if the model name doesn't match what's actually loaded — it simply processes the request using whatever model is currently loaded in LM Studio. This means if you have two LM Studio models configured (e.g., `qwen/qwen3-8b` and `nvidia/nemotron-3-nano`) and only one is loaded, both will show as "Connected successfully" even though only one is actually available.

## Goal

The provider connection test for LM Studio should verify that the **configured model is actually loaded** before reporting success.

## Requirements

- Before sending the chat completion request, query LM Studio's native API (`/api/v0/models`) to get the list of available models and their loaded states
- Verify the configured model has state `"loaded"` 
- Only return success if:
  1. The endpoint is reachable
  2. The configured model exists in the LM Studio model list
  3. The configured model has `"state": "loaded"`
- Return an appropriate error message if the configured model is not loaded (e.g., "Model 'xyz' is not loaded in LM Studio. Please load it first.")
- Continue to also test the chat endpoint as before (to verify the model actually responds), but only after confirming it's loaded

## Files to Modify

- `buddy-server/src/api/config.rs` — update the `test_provider` function to check loaded state for LM Studio providers

## Acceptance Criteria

- [x] LM Studio provider connection test fails with a clear message when the configured model is not loaded
- [x] LM Studio provider connection test succeeds only when the configured model is loaded
- [x] The chat completion request is still sent (to verify the model responds)
- [x] All existing tests pass (`cargo test`)

## Test Cases

- [x] Configure LM Studio with model "qwen3-8b"; mock LM Studio API returning "qwen3-8b" as loaded; assert test returns success
- [x] Configure LM Studio with model "nemotron-3-nano"; mock LM Studio API returning "qwen3-8b" as loaded (not nemotron); assert test returns error with message about model not loaded
- [x] Configure LM Studio with model "qwen3-8b"; mock LM Studio API unreachable; assert test returns connection error
