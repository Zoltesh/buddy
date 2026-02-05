# 012 — Conversation Management API

## Description

Add REST API endpoints for creating, listing, loading, and deleting conversations. Update the existing `POST /api/chat` endpoint to work within the context of a persisted conversation.

## Goal

The frontend (and any future client) can manage conversations through a clean REST API. Chat messages are automatically persisted to the conversation they belong to.

## Requirements

- **`GET /api/conversations`** — returns a JSON array of conversation summaries:
  ```json
  [{ "id": "uuid", "title": "string", "created_at": "iso8601", "updated_at": "iso8601", "message_count": 0 }]
  ```
  Ordered by `updated_at` descending (most recent first)
- **`POST /api/conversations`** — creates a new empty conversation, returns the conversation object
- **`GET /api/conversations/:id`** — returns a single conversation with all its messages
- **`DELETE /api/conversations/:id`** — deletes a conversation and all its messages; returns 204
- **`POST /api/chat`** — updated to accept an optional `conversation_id`:
  ```json
  { "conversation_id": "uuid-or-null", "messages": [...] }
  ```
  - If `conversation_id` is provided: load conversation history from the store, append the new user message, run the chat completion, persist all new messages (user, assistant, tool calls, tool results)
  - If `conversation_id` is `null` or omitted: auto-create a new conversation, persist everything, return the `conversation_id` in the SSE stream (as a metadata event)
- New SSE event: `ChatEvent::ConversationMeta { conversation_id: String }` — sent as the first event so the frontend knows which conversation it's in
- Add the `Store` to `AppState` so all handlers can access it
- All API errors continue to return structured JSON with `code` and `message`

## Acceptance Criteria

- [ ] `GET /api/conversations` returns an empty array initially, then lists created conversations
- [ ] `POST /api/conversations` creates and returns a new conversation
- [ ] `GET /api/conversations/:id` returns the conversation with all messages
- [ ] `GET /api/conversations/:nonexistent` returns 404 with structured error
- [ ] `DELETE /api/conversations/:id` removes the conversation and returns 204
- [ ] `POST /api/chat` without `conversation_id` auto-creates a conversation and emits `ConversationMeta`
- [ ] `POST /api/chat` with `conversation_id` appends to the existing conversation
- [ ] All messages (user, assistant, tool calls, tool results) are persisted after a chat request
- [ ] Existing chat functionality is not broken

## Test Cases

- `GET /api/conversations` on fresh database: assert empty array
- `POST /api/conversations`, then `GET /api/conversations`: assert the new conversation appears
- `POST /api/chat` with no `conversation_id`, then `GET /api/conversations`: assert a conversation was auto-created with a title from the first message
- `POST /api/chat` with a valid `conversation_id`, then `GET /api/conversations/:id`: assert the new messages are persisted
- `DELETE /api/conversations/:id`, then `GET /api/conversations/:id`: assert 404
- `POST /api/chat` with a nonexistent `conversation_id`: assert 404 error
- Assert SSE stream starts with a `ConversationMeta` event containing the conversation ID
