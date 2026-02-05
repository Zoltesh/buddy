# 006 — Chat UI

## Description

Build the Svelte + Tailwind web interface for a single-conversation chat. The UI connects to the backend API and renders streamed responses in real time.

## Goal

A clean, minimal chat interface where a user can type a message, send it, and see the assistant's response stream in token by token.

## Requirements

- A single-page Svelte app with:
  - A message list showing the conversation (user messages right-aligned, assistant messages left-aligned)
  - A text input with a send button at the bottom
  - Auto-scroll to the latest message
- Connects to the backend streaming endpoint (WebSocket or SSE, matching task 005)
- Assistant responses render incrementally as tokens arrive (not after the full response)
- Basic loading state: a visual indicator while the assistant is generating
- Tailwind for all styling — no custom CSS files
- Responsive layout that works on desktop and mobile viewports
- Markdown rendering for assistant messages (basic: bold, italic, code blocks, lists)
- No conversation persistence in the UI — refresh clears the chat (matches V0.1 scope)

## Acceptance Criteria

- [ ] User can type a message and press Enter (or click Send) to submit
- [ ] Assistant response appears token-by-token as it streams
- [ ] Conversation history is displayed correctly for multi-turn exchanges
- [ ] The UI is usable at 360px and 1440px viewport widths
- [ ] Code blocks in assistant responses are rendered with monospace font and visible boundaries
- [ ] The input is disabled while a response is streaming; re-enabled after `Done`
- [ ] No JavaScript errors in the browser console during normal use

## Test Cases

- Send a message; verify tokens render incrementally (not all at once)
- Send 10 messages in a row; verify scroll stays at the bottom
- Resize to 360px width; verify layout doesn't break
- Refresh the page; verify the conversation is gone (no persistence)
