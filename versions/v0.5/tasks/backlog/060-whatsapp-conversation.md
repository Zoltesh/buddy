# 060 — WhatsApp Conversation Flow

## Description

With the WhatsApp crate set up (task 059), this task implements the conversation flow: receiving a user message via webhook, routing it through buddy-core's provider chain, and sending the response back via the WhatsApp API. Each WhatsApp sender (identified by phone number) maps to a buddy conversation. Messages are stored in the shared SQLite database.

## Goal

Users can have full text conversations with buddy through WhatsApp. Messages and responses are stored alongside web and Telegram conversations.

## Requirements

- On receiving a text message via the `POST /webhook` handler:
  1. Look up or create a buddy conversation for the sender's phone number
     - Mapping: sender phone number → `conversation_id` (persisted in database)
     - New chats get a conversation created via `Store::create_conversation_with_source(title, "whatsapp")` (from task 057)
     - Conversation title: first message text, truncated to 50 chars
  2. Append the user message to the conversation
  3. Load conversation history
  4. Call the provider chain
  5. Collect the full response (consume token stream, concatenate text)
  6. Append the assistant message to the conversation
  7. Send the response via `WhatsAppClient::send_text_message()`
- **Response processing must be async and non-blocking**: the `POST /webhook` handler must return 200 immediately. Process the message in a spawned task (`tokio::spawn`)
  - WhatsApp has a strict response time requirement for webhooks
- **Response formatting:**
  - WhatsApp supports basic formatting: *bold*, _italic_, ~strikethrough~, ```code```
  - Convert markdown to WhatsApp format where possible
  - If response exceeds 4096 characters, split into multiple messages
- **Error handling:**
  - Provider errors: send "Sorry, I couldn't process that. Please try again."
  - WhatsApp API errors when sending: log error, do not retry
- **Duplicate message filtering:**
  - WhatsApp sometimes sends the same webhook event multiple times
  - Track `message_id` from the webhook payload to skip duplicates
  - Use a simple in-memory set with TTL (5 minutes) — no database table needed
- Do not implement tool execution over WhatsApp in this task (task 061)
  - If provider requests a tool call: send "I wanted to use a tool ({tool_name}), but tool execution is not yet available over WhatsApp."
- Construct `AppState` from `buddy-core` in the WhatsApp binary's startup

## Acceptance Criteria

- [ ] Text messages from WhatsApp are processed through the provider chain
- [ ] Responses are sent back via the WhatsApp Business API
- [ ] Each sender phone number maps to a persistent buddy conversation
- [ ] Messages are stored in the SQLite database
- [ ] Conversations created from WhatsApp have source `"whatsapp"`
- [ ] Webhook handler returns 200 immediately (processing is async)
- [ ] Duplicate messages (same message_id) are ignored
- [ ] Responses exceeding 4096 characters are split into multiple messages
- [ ] Provider errors result in user-friendly WhatsApp messages
- [ ] Tool calls are gracefully declined with an informational message
- [ ] All existing tests pass

## Test Cases

- [ ] Receive a text message from a new sender; assert a conversation is created with source `"whatsapp"` and the message is stored
- [ ] Receive a text message; assert the provider chain is called and a response is sent via `WhatsAppClient`
- [ ] Receive two messages from the same sender; assert both are in the same conversation
- [ ] Receive the same message_id twice; assert only the first is processed
- [ ] Receive a message that triggers a tool call; assert the response is an informational message about tool unavailability
- [ ] Trigger a provider error; assert the WhatsApp reply is a user-friendly error message
- [ ] Assert the `POST /webhook` handler returns 200 within 100ms (processing happens asynchronously)
