# 043 — Embedder Health Check Endpoint

## Description

The embedding design (see `versions/v0.5/embedding_design.md`) requires the system to detect when an external embedder goes down and report it to the user. This task adds a health check endpoint that tests whether the active embedder can produce embeddings. For the built-in local embedder, this always succeeds. For external embedders (e.g., OpenAI embeddings), this attempts a real embedding request and reports the result. The frontend uses this to show accurate status in the embedding settings card and to trigger warning banners when memory features are degraded.

## Goal

The frontend can query a single endpoint to determine whether the active embedder is healthy, what model it uses, and whether it is the built-in local or an external provider.

## Requirements

- Add `GET /api/embedder/health` endpoint
- Response body (200 OK, always — health status is in the payload):
  ```json
  {
    "active": true,
    "provider_type": "local",
    "model_name": "all-MiniLM-L6-v2",
    "dimensions": 384,
    "status": "healthy",
    "message": null
  }
  ```
  - `active`: always `true` after task 042 (local embedder is always available)
  - `provider_type`: `"local"` for built-in, or the provider type string (e.g., `"openai"`) for external
  - `model_name`: the embedder's model name string
  - `dimensions`: the embedder's dimension count
  - `status`: `"healthy"` or `"unhealthy"`
  - `message`: `null` when healthy, a human-readable error string when unhealthy
- Health check logic:
  - Call `embedder.embed(&["health check"])` with a trivial input
  - If it returns `Ok` with a vector of the expected dimensions → `"healthy"`
  - If it returns `Err` → `"unhealthy"` with the error message
- The health check must complete within 5 seconds (timeout)
- The endpoint must not modify any state
- Add a `provider_type()` method to the `Embedder` trait that returns `&str` (e.g., `"local"`, `"openai"`). Implement it on `LocalEmbedder`. This is the only trait change allowed
- Register the route in `main.rs`

## Acceptance Criteria

- [x] `GET /api/embedder/health` returns 200 with the specified JSON structure
- [x] When the local embedder is active, `status` is `"healthy"` and `provider_type` is `"local"`
- [x] When an external embedder is configured and reachable, `status` is `"healthy"` with the correct `provider_type`
- [x] When an external embedder is configured but unreachable, `status` is `"unhealthy"` with a descriptive `message`
- [x] The `Embedder` trait has a `provider_type()` method
- [x] The endpoint responds within 5 seconds even if the external embedder hangs
- [x] No state is modified by the health check

## Test Cases

- [x] Call `GET /api/embedder/health` with the local embedder active; assert 200, `status: "healthy"`, `provider_type: "local"`, `model_name: "all-MiniLM-L6-v2"`, `dimensions: 384`
- [x] Call `GET /api/embedder/health` with a mock embedder that returns `Err`; assert 200, `status: "unhealthy"`, `message` is non-null and contains an error description
- [x] Call `GET /api/embedder/health` with a mock embedder that returns a vector of the wrong dimensions; assert 200, `status: "unhealthy"` (dimension mismatch is detected)
- [x] Assert the health check endpoint does not call `VectorStore::store()` or modify any database state
- [x] Assert the health check response includes the correct `dimensions` value matching the active embedder
