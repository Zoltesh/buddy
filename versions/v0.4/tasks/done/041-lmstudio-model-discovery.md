# 041 — LM Studio Model Discovery

## Description

When configuring an LM Studio provider in the Settings UI, users must manually type the exact model identifier. If it doesn't match what's loaded in LM Studio, requests silently route to the wrong model (or the only loaded model). Add a "fetch available models" feature that queries the LM Studio instance and presents discovered models for selection.

## Goal

Users can see which models are available (and which are loaded) on their LM Studio instance directly from the Settings UI, eliminating guesswork and preventing model identifier mismatches.

## Requirements

- Add a backend endpoint (e.g. `POST /api/config/discover-models`) that takes a provider endpoint URL and queries it for available models
- For LM Studio providers, query `GET {endpoint}/../api/v0/models` (the native LM Studio REST API) which returns model metadata including load state, quantization, and context length
- Fall back to `GET {endpoint}/models` (OpenAI-compatible) if the native endpoint is unavailable — this returns less metadata but still provides model identifiers
- Return a structured list of discovered models:
  ```json
  {
    "models": [
      {
        "id": "qwen/qwen3-8b",
        "loaded": true,
        "context_length": 8192
      }
    ]
  }
  ```
- The request must have a short timeout (5 seconds) to avoid blocking the UI
- If the endpoint is unreachable, return a clear error message
- In the frontend Models settings, add a "Discover Models" button next to the model text input for LM Studio providers
- Clicking it queries the backend, then shows the results as a selectable list (or populates a dropdown)
- Selecting a model fills in the model identifier field

## Acceptance Criteria

- [x] Backend endpoint accepts a provider endpoint URL and returns discovered models
- [x] Native LM Studio API is tried first, with OpenAI-compatible `/models` as fallback
- [x] Each model entry includes at minimum its identifier and loaded state (when available)
- [x] Unreachable endpoint returns `{ "status": "error", "message": "..." }`
- [x] Request times out after 5 seconds
- [x] Frontend shows a "Discover Models" button for LM Studio provider entries
- [x] Clicking the button fetches and displays available models
- [x] Selecting a discovered model populates the model field
- [x] Existing manual text entry still works (discovery is optional, not required)

## Test Cases

- [x] POST discover-models with a valid LM Studio endpoint (mock); assert response contains model list with identifiers
- [x] POST discover-models with an unreachable endpoint; assert `status: "error"` with connection error message
- [x] POST discover-models with an endpoint that only supports OpenAI-compatible `/models`; assert fallback works and returns model identifiers
- [x] Assert the request completes within the timeout even if the endpoint hangs
- [x] Assert no config state is modified by discovery (read-only operation)
