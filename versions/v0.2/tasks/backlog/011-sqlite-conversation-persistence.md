# 011 — SQLite Conversation Persistence

## Description

Add SQLite-backed storage for conversations. Every message — including tool calls and tool results — is persisted so conversations survive server restarts and can be listed and resumed.

## Goal

Conversations are durable. The user can close their browser, restart the server, and pick up exactly where they left off. The storage layer is a clean abstraction that the API layer calls into.

## Requirements

- Add `rusqlite` dependency with `bundled` feature (no external SQLite install needed)
- Database file location configurable in `buddy.toml`:
  ```toml
  [storage]
  database = "buddy.db"  # default
  ```
- Database schema:
  ```sql
  CREATE TABLE conversations (
      id TEXT PRIMARY KEY,
      title TEXT NOT NULL,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL
  );

  CREATE TABLE messages (
      id TEXT PRIMARY KEY,
      conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
      role TEXT NOT NULL,
      content_type TEXT NOT NULL,
      content_json TEXT NOT NULL,
      timestamp TEXT NOT NULL,
      sort_order INTEGER NOT NULL
  );
  ```
- A `Store` struct providing:
  - `create_conversation(title) -> Conversation`
  - `list_conversations() -> Vec<ConversationSummary>` (id, title, created_at, updated_at, message count)
  - `get_conversation(id) -> Option<Conversation>` (with all messages)
  - `delete_conversation(id)`
  - `append_message(conversation_id, message)` — persists a single message
  - `update_conversation_title(id, title)`
- Auto-run migrations on startup (create tables if they don't exist)
- Conversation IDs are UUIDs
- Conversation title auto-generated from the first user message (first 80 chars, truncated at word boundary)
- The `Store` is `Send + Sync` (wraps `Connection` in a `Mutex` or uses a connection pool)

## Acceptance Criteria

- [ ] Database file is created on first startup at the configured path
- [ ] `create_conversation` returns a conversation with a valid UUID
- [ ] `append_message` persists messages that survive a `Store` reinstantiation (simulating restart)
- [ ] `list_conversations` returns conversations ordered by `updated_at` descending
- [ ] `get_conversation` returns all messages in correct order
- [ ] `delete_conversation` removes the conversation and all its messages
- [ ] Tool call and tool result messages are stored and retrieved correctly
- [ ] Schema migrations run idempotently (safe to run on an existing database)
- [ ] Default database path works when `[storage]` is omitted from config

## Test Cases

- Create a conversation, append 3 messages, drop the `Store`, reopen the database, call `get_conversation`: assert all 3 messages are present and in order
- Create 3 conversations, call `list_conversations`: assert all 3 are returned, ordered by most recently updated
- Delete a conversation, call `get_conversation`: assert `None`
- Append a `ToolCall` message and a `ToolResult` message; retrieve them; assert content round-trips correctly
- Create a conversation with a first message "Tell me about the history of computing in the modern era and how it has...": assert title is truncated to ~80 chars at a word boundary
- Open the database twice (simulating restart), assert no migration errors
