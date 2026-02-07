# 025 — Recall Skill (Long-Term Memory Search)

## Description

Implement the `recall` skill that explicitly searches long-term memory for relevant stored information. The skill embeds the query and performs a similarity search against the vector store.

## Goal

The LLM can explicitly search its long-term memory to find previously stored facts, preferences, or context relevant to the current conversation.

## Requirements

- Implement a `recall` skill:
  - Input schema: `{ "query": "...", "limit": N }`
    - `query` (required): the search text
    - `limit` (optional, default 5): maximum number of results to return
  - On execution:
    1. Embed the query text using the configured `Embedder`
    2. Search the `VectorStore` with the query embedding and limit
    3. Format and return the results
  - If no embedder is configured, return `SkillError::ExecutionFailed` with a message explaining that an embedding model is required
  - If the vector store is empty, return a message indicating no memories are stored
- Result format returned to the LLM:
  ```json
  {
    "results": [
      {
        "text": "User's favorite color is blue",
        "category": "preference",
        "score": 0.92,
        "created_at": "2025-01-15T10:30:00Z"
      }
    ],
    "total_found": 1
  }
  ```
- The skill needs access to the `Embedder` and `VectorStore` from `AppState`
- Register the skill in `SkillRegistry` (only when embedder and vector store are available)

## Acceptance Criteria

- [x] `recall` skill embeds the query and searches the vector store
- [x] Results are returned ordered by similarity score (highest first)
- [x] The `limit` parameter caps the number of results
- [x] Default limit is 5 when not specified
- [x] Calling `recall` without an embedding model returns a clear error
- [x] Searching an empty store returns an empty results list (not an error)
- [x] The skill appears in `tool_definitions()` with correct schema
- [x] Results include source text, category, score, and timestamp

## Test Cases

- [x] Store 3 memories via `remember`; call `recall` with a related query; assert relevant memories are returned with scores
- [x] Call `recall` with `limit: 1`; assert only 1 result is returned
- [x] Call `recall` with `limit` omitted; assert default limit of 5 is used
- [x] Call `recall` on an empty vector store; assert empty results, no error
- [x] Call `recall` when no embedder is configured; assert `SkillError::ExecutionFailed`
- [x] Call `recall` with empty `query`; assert `SkillError::InvalidInput`
- [x] Store memories about "cooking" and "programming"; recall "food recipes"; assert cooking-related memories rank higher
