# 013 — Conversation UI and Tool-Call Rendering

## Description

Update the frontend to support multiple conversations (list, create, switch) and render tool-call activity visibly in the chat. The user sees what buddy did, not just the final answer.

## Goal

A user can manage multiple conversations and see full transparency into buddy's tool usage. Conversations persist across page refreshes.

## Requirements

- **Conversation sidebar:**
  - Left sidebar listing all conversations (title + relative timestamp)
  - "New Chat" button at the top
  - Clicking a conversation loads its message history
  - Active conversation is visually highlighted
  - Sidebar is collapsible on mobile (hamburger menu)
- **Conversation lifecycle:**
  - Starting a new chat creates a conversation on the first message send
  - The conversation title updates from the `ConversationMeta` event or first user message
  - Conversations load from `GET /api/conversations` on app mount
  - Switching conversations fetches full history from `GET /api/conversations/:id`
  - Deleting a conversation (e.g., swipe or context menu) calls `DELETE` and removes it from the list
- **Tool-call rendering:**
  - Tool calls appear as distinct visual blocks in the message stream (not inline with text)
  - Each tool-call block shows: skill name, a summary of what was done, and an expandable section with full input/output
  - Visual styling: muted/subtle container (e.g., gray border, icon) to distinguish from regular messages
  - Tool calls appear between the assistant messages that triggered them and the response that followed
- **Updated SSE handling:**
  - Handle `ConversationMeta`, `ToolCallStart`, and `ToolCallResult` events
  - Store conversation ID from `ConversationMeta` and include it in subsequent requests
- Responsive layout: sidebar becomes a slide-out drawer on viewports < 768px

## Acceptance Criteria

- [ ] Conversation list loads on app start and displays all conversations
- [ ] Clicking "New Chat" clears the message area and starts a fresh conversation
- [ ] Clicking a conversation loads and displays its history
- [ ] Conversations persist: refresh the page, conversations are still listed
- [ ] Tool-call blocks appear in the chat showing skill name and result
- [ ] Tool-call details are expandable (collapsed by default)
- [ ] Deleting a conversation removes it from the sidebar
- [ ] The layout is responsive — sidebar collapses on mobile
- [ ] No JavaScript errors during normal conversation and tool-call flows

## Test Cases

- Start the app with existing conversations in the database; assert the sidebar lists them
- Click "New Chat", send a message; assert a new conversation appears in the sidebar
- Click an existing conversation; assert its messages load correctly
- Trigger a tool call (e.g., ask buddy to read a file); assert the tool-call block renders with skill name and result
- Expand a tool-call block; assert full input/output is visible
- Delete a conversation from the sidebar; assert it disappears and the view resets
- Resize to 360px; assert the sidebar collapses and a hamburger menu appears
- Refresh the page; assert all conversations are still listed
