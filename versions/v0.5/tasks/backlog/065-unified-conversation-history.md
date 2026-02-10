# 065 — Unified Conversation History

## Description

buddy now has multiple interfaces (web, Telegram, WhatsApp) that all write to the same SQLite database. Task 057 added a `source` column to conversations. This task completes the unified history experience: the web UI shows all conversations regardless of source, displays where each conversation originated, and allows continuing any conversation from the web interface. This means a user can start a chat on Telegram, then pick it up in the browser with full history.

## Goal

The web UI's conversation list shows conversations from all interfaces with source badges, and users can continue any conversation from the web.

## Requirements

- **Conversation list updates (Sidebar.svelte):**
  - Each conversation in the sidebar shows a small source badge:
    - Web: no badge (default, to avoid visual noise for the common case)
    - Telegram: small "TG" badge with a distinct color (e.g., blue)
    - WhatsApp: small "WA" badge with a distinct color (e.g., green)
  - Badge appears next to the conversation title
  - Conversations from all sources are listed together, sorted by `updated_at` (most recent first) — this is already the default sort
- **Conversation detail:**
  - When opening a conversation that originated from Telegram or WhatsApp, the chat view works normally
  - Messages display the same way regardless of source
  - The user can send new messages, which are appended to the conversation and processed by the provider chain as usual
  - A subtle header note appears at the top of the conversation: "This conversation started on Telegram" or "This conversation started on WhatsApp" — only for non-web conversations
- **API updates:**
  - `GET /api/conversations` already returns conversations — ensure the `source` field is included in the response
  - `GET /api/conversations/{id}` already returns a full conversation — ensure `source` is included
  - Add `source` to the `Conversation` and `ConversationSummary` serialized output if not already present
  - No new endpoints needed
- **Conversation continuation:**
  - When a user sends a message in the web UI to a conversation that originated on Telegram/WhatsApp, it is processed normally through the provider chain
  - The response is stored in the database but is NOT sent back to the original interface (no cross-interface push notification — that is future work)
  - This is a one-directional continuation: web can continue any conversation, but Telegram/WhatsApp users continue in their own interface
- **No backend changes to the Store** beyond ensuring `source` is serialized. Task 057 already added the column and creation method.

## Acceptance Criteria

- [ ] Conversation list shows conversations from all sources (web, telegram, whatsapp)
- [ ] Telegram conversations show a "TG" badge in the sidebar
- [ ] WhatsApp conversations show a "WA" badge in the sidebar
- [ ] Web conversations show no badge
- [ ] Opening a non-web conversation shows the full message history
- [ ] A header note indicates the conversation's original source for non-web conversations
- [ ] Users can send new messages in conversations from any source
- [ ] New messages are processed by the provider chain as usual
- [ ] `GET /api/conversations` response includes the `source` field
- [ ] `GET /api/conversations/{id}` response includes the `source` field
- [ ] All existing conversation functionality works (create, delete, send messages)

## Test Cases

- [ ] Create a conversation with source "web" and one with source "telegram"; call `GET /api/conversations`; assert both appear in the list with correct `source` field
- [ ] Load the web UI with a Telegram conversation in the database; sidebar shows the conversation with a "TG" badge
- [ ] Load the web UI with a WhatsApp conversation in the database; sidebar shows the conversation with a "WA" badge
- [ ] Load the web UI with only web conversations; no badges are shown
- [ ] Click on a Telegram conversation in the sidebar; chat view loads with full message history and a "This conversation started on Telegram" header note
- [ ] Send a message in a Telegram-originated conversation via the web UI; message is processed by the provider and response appears in the chat
- [ ] After sending a message in a Telegram-originated conversation, the conversation's `updated_at` is refreshed and it moves to the top of the list
- [ ] Create a new conversation via the web UI; it has source "web" and no badge
