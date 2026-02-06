# 026 — Automatic Context Retrieval

## Description

On each user message, automatically search long-term memory for relevant past interactions and inject them as system context before sending the conversation to the LLM. The retrieved context is visible to the user for transparency.

## Goal

buddy proactively recalls relevant memories without the LLM needing to explicitly use the `recall` skill, providing continuity across conversations. The user can see what was recalled and can disable the feature.

## Requirements

- Before each LLM call in the chat handler, if an embedder and vector store are available:
  1. Embed the user's latest message
  2. Search the vector store for the top N most relevant memories (N configurable, default 3)
  3. If results meet a minimum similarity threshold (e.g. 0.5), inject them into the system prompt as additional context
- Injected context format in the system message:
  ```
  ## Recalled Memories
  - "User's favorite color is blue" (preference, relevance: 0.92)
  - "User is working on a Rust web server project" (fact, relevance: 0.78)
  ```
- Add a new `ChatEvent` variant to surface retrieved context to the frontend:
  - `ChatEvent::MemoryContext { memories: Vec<MemorySnippet> }` where `MemorySnippet` has `text`, `category`, and `score`
  - Emitted before `TokenDelta` events so the frontend can display what was recalled
- Automatic retrieval can be disabled:
  - Per-conversation: via a flag in the `ChatRequest` (e.g. `"disable_memory": true`)
  - Globally: via a config option (e.g. `[memory] auto_retrieve = false`)
- If no embedder or vector store is configured, skip retrieval silently (no error)
- The similarity threshold prevents injecting irrelevant memories (low-scoring results are discarded)
- Retrieval does not slow down the response noticeably — embedding a single query is fast

## Acceptance Criteria

- [ ] User messages trigger automatic memory search when embedder and vector store are available
- [ ] Relevant memories are injected into the system prompt before the LLM call
- [ ] A `ChatEvent::MemoryContext` event is emitted with retrieved memories
- [ ] Results below the similarity threshold are not injected
- [ ] `disable_memory: true` in the chat request skips retrieval for that request
- [ ] Global config option disables automatic retrieval
- [ ] No errors or delays when embedder/vector store are unavailable
- [ ] Existing chat behavior is unchanged when no memories are stored

## Test Cases

- Store a memory "User likes Python"; send message "What programming language should I learn?"; assert memory is retrieved and injected into context
- Store a memory with low relevance to the query; assert it is NOT injected (below threshold)
- Send a chat request with `disable_memory: true`; assert no memory retrieval occurs
- Set config `[memory] auto_retrieve = false`; send a message; assert no memory retrieval occurs
- Send a message with no embedder configured; assert chat works normally with no retrieval
- Send a message with an empty vector store; assert chat works normally with no injected context
- Assert `ChatEvent::MemoryContext` appears in the SSE stream before `TokenDelta` events
- Assert the injected system prompt section contains the recalled memory text
