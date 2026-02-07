# 039 — Chat UI Warning Banners with Settings Links

## Description

Display non-intrusive warning banners in the chat UI when buddy is operating in a degraded state. Banners include actionable messages with links to the relevant Settings section. This is the frontend companion to the warning system implemented in task 027.

## Goal

Users are clearly informed in the chat interface when features are degraded, with one-click navigation to fix the issue in Settings.

## Requirements

- On chat page load, fetch warnings from `GET /api/warnings` (or use the `ChatEvent::Warnings` from the SSE stream)
- Display warnings as a non-intrusive banner bar at the top of the chat area (below the header, above messages)
- Banner styling by severity:
  - `Warning` severity: yellow/amber background with a warning icon
  - `Info` severity: blue/gray background with an info icon
- Each banner shows the warning's human-readable `message`
- Banners include a clickable link to the relevant Settings section:
  - `no_embedding_model` → link to `/settings` (Models > Embedding section)
  - `no_vector_store` → link to `/settings` (Models > Embedding section)
  - `single_chat_provider` → link to `/settings` (Models > Chat section)
  - `embedding_dimension_mismatch` → link to `/settings` (Models > Embedding section)
- Warning-to-section mapping is based on the warning `code` field
- Banners are dismissible (X button) for the current session, but reappear if the page is reloaded and the issue persists
- Banners stack vertically if multiple warnings are active
- Banners do not cover or push aside chat messages in a disruptive way — they should be compact (one line per warning, or collapsible)
- Update banners in real-time: if a `ChatEvent::Warnings` event arrives during a chat stream with updated warnings, refresh the banner state
- When no warnings are active, no banner space is rendered

## Acceptance Criteria

- [ ] Warnings from `GET /api/warnings` are displayed as banners in the chat UI
- [ ] Warning severity determines banner color (yellow for Warning, blue for Info)
- [ ] Each banner includes a link to the relevant Settings section
- [ ] Banners are dismissible for the current session
- [ ] Dismissed banners reappear on page reload if the issue persists
- [ ] Multiple warnings stack vertically
- [ ] Banners update when new `ChatEvent::Warnings` events arrive
- [ ] No banner space is rendered when there are no warnings
- [ ] Banner links navigate to the correct Settings section

## Test Cases

- [ ] Load chat with a `no_embedding_model` warning active; assert a yellow banner appears with a message about embedding and a link to Settings
- [ ] Load chat with a `single_chat_provider` info; assert a blue/gray banner appears with a link to Settings > Models
- [ ] Load chat with no warnings; assert no banner is rendered
- [ ] Dismiss a warning banner; assert it disappears; reload the page; assert it reappears
- [ ] Load chat with multiple warnings; assert all are displayed as stacked banners
- [ ] Start a chat that emits a `ChatEvent::Warnings` event; assert banners update to reflect the new warnings
- [ ] Click the Settings link in a banner; assert navigation to the Settings page
