# 024 — Remember Skill (Long-Term Memory Write)

## Description

Implement the `remember` skill that explicitly saves a fact or preference to long-term memory. The skill embeds the text and stores it in the vector store for later retrieval.

## Goal

The LLM can explicitly commit important information to persistent long-term memory that survives across conversations.

## Requirements

- Implement a `remember` skill:
  - Input schema: `{ "text": "...", "category": "..." }`
    - `text` (required): the fact, preference, or information to remember
    - `category` (optional): a label for organizing memories (e.g. `"preference"`, `"fact"`, `"project"`)
  - On execution:
    1. Embed the text using the configured `Embedder`
    2. Store the embedding, source text, and metadata (category, timestamp, conversation_id) in the `VectorStore`
    3. Return a confirmation message
  - If no embedder is configured, return `SkillError::ExecutionFailed` with a message explaining that an embedding model is required
- The skill needs access to the `Embedder` and `VectorStore` from `AppState`
  - Pass these as dependencies when constructing the skill (or access via a shared context)
- Generate a unique ID for each memory entry (UUID)
- Metadata stored alongside the embedding:
  ```json
  {
    "category": "preference",
    "conversation_id": "abc-123",
    "created_at": "2025-01-15T10:30:00Z"
  }
  ```
- Register the skill in `SkillRegistry` (only when embedder and vector store are available)

## Acceptance Criteria

- [ ] `remember` skill embeds text and stores it in the vector store
- [ ] Stored entries include source text, embedding, and metadata
- [ ] Category is stored in metadata when provided
- [ ] Calling `remember` without an embedding model returns a clear error
- [ ] Each memory gets a unique ID
- [ ] The skill appears in `tool_definitions()` with correct schema
- [ ] Stored memories are retrievable via the vector store's `search` method

## Test Cases

- Call `remember` with `{ "text": "User's favorite color is blue" }`; search the vector store for "favorite color"; assert the entry is found
- Call `remember` with `{ "text": "...", "category": "preference" }`; assert stored metadata includes `"category": "preference"`
- Call `remember` twice with different texts; assert both entries exist in the vector store
- Call `remember` when no embedder is configured; assert `SkillError::ExecutionFailed` with a message about embedding
- Call `remember` with empty `text`; assert `SkillError::InvalidInput`
- Assert the stored entry's source text matches the input text exactly
