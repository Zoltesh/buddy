# 057 — Telegram Conversation Flow

## Description

With the Telegram crate set up (task 056), this task implements the actual conversation flow: receiving a user message, routing it through buddy-core's provider chain, and sending the response back as a Telegram message. Each Telegram chat (identified by `chat_id`) maps to a buddy conversation. Messages are stored in the shared SQLite database, so conversations started on Telegram are visible in the web UI.

## Goal

Users can have full text conversations with buddy through Telegram. Messages and responses are stored in the conversation database alongside web conversations.

## Requirements

- On receiving a text message from Telegram:
  1. Look up or create a buddy conversation for the Telegram `chat_id`
     - Use a mapping: `chat_id` → `conversation_id` (store this in the database or an in-memory map persisted to SQLite)
     - New chats get a conversation created via `Store::create_conversation()`
     - Conversation title: first message text, truncated to 50 chars
  2. Append the user message to the conversation via `Store::append_message()`
  3. Load the conversation history
  4. Call the provider chain: `provider.complete(messages, tools)`
  5. Collect the full response (consume the token stream, concatenate text tokens)
  6. Append the assistant message to the conversation
  7. Send the response text as a Telegram reply
- **Response formatting:**
  - Telegram supports a subset of Markdown (bold, italic, code, links)
  - Send responses with `ParseMode::MarkdownV2` (escape special characters as needed)
  - If the response exceeds Telegram's 4096 character limit, split into multiple messages
- **Error handling:**
  - If the provider returns an error: send a user-friendly message like "Sorry, I couldn't process that. Please try again."
  - If all providers are unavailable: send "All models are currently unavailable. Please check your buddy configuration."
  - Log full error details server-side
- **Conversation source tracking:**
  - Add a `source` column to the `conversations` table in `Store`: `TEXT NOT NULL DEFAULT 'web'`
  - Values: `"web"`, `"telegram"` (more added in future tasks)
  - Web conversations continue to default to `"web"`
  - Telegram conversations are created with source `"telegram"`
  - Add `Store::create_conversation_with_source(title, source)` method
  - Add `source` field to `ConversationSummary` struct
- Do not implement tool execution / skill calling over Telegram in this task (that is task 058)
  - If the provider requests a tool call: send a message "I wanted to use a tool ({tool_name}), but tool execution is not yet available over Telegram."
- Construct `AppState` from `buddy-core` in the Telegram binary's startup

## Acceptance Criteria

- [x] Text messages from Telegram are processed through the provider chain
- [x] Responses are sent back as Telegram messages
- [x] Each Telegram chat_id maps to a persistent buddy conversation
- [x] Messages are stored in the SQLite database
- [x] Conversations created from Telegram have source `"telegram"`
- [x] Web conversations have source `"web"` (default, no change to existing behavior)
- [x] `ConversationSummary` includes the `source` field
- [x] Responses exceeding 4096 characters are split into multiple messages
- [x] Provider errors result in user-friendly Telegram messages
- [x] Tool calls are gracefully declined with an informational message
- [x] All existing tests pass

## Test Cases

- [x] Send a text message from a new Telegram chat; assert a conversation is created in the database with source `"telegram"` and the message is stored
- [x] Send a text message; assert the provider chain is called with the conversation history and a response is returned
- [x] Send two messages from the same chat_id; assert both messages are in the same conversation
- [x] Send a message that triggers a tool call; assert the response is an informational message about tool unavailability (not an error)
- [x] Trigger a provider error; assert the Telegram reply is a user-friendly error message, not a raw error
- [x] Create a conversation via the web API; assert its source is `"web"`
- [x] Create a conversation via Telegram; list conversations via `Store::list_conversations()`; assert the source field is `"telegram"`
- [x] Existing conversations created before the migration have source `"web"` (default column value)
