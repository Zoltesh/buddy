# 081 — Add Missing ARIA Labels to Frontend

## Description

Several interactive elements in the frontend lack proper ARIA attributes, making them inaccessible to screen reader users. The main gaps are in the Sidebar and Chat components.

## Goal

All interactive elements (buttons, clickable items, toggles) have appropriate ARIA labels so the UI is usable with a screen reader.

## Requirements

### Sidebar (`frontend/src/lib/Sidebar.svelte`)

1. **Conversation list items** — Each conversation item acts as a button (has `role="button"` or click handler) but lacks an `aria-label`. Add `aria-label="Open conversation: {title}"` (where `{title}` is the conversation title or a fallback like "Untitled conversation").

2. **Delete conversation button** (line ~156-165) — The delete button inside each conversation item has no `aria-label`. Add `aria-label="Delete conversation"`.

3. **New conversation button** — Verify it has an `aria-label`. If it only has an icon, add `aria-label="New conversation"`.

4. **Sidebar collapse/expand button** — Verify it has an `aria-label` that reflects its state (e.g., `aria-label="Collapse sidebar"` / `aria-label="Expand sidebar"`).

### Chat (`frontend/src/lib/Chat.svelte`)

5. **Send button** (line ~386-395) — If this button only contains an icon (no visible text), add `aria-label="Send message"`.

6. **Message input** — Verify the textarea/input has an `aria-label` like `"Type a message"` or a visible `<label>`.

### General rules

- Only add `aria-label` where the element lacks visible text that describes its purpose
- Buttons with visible text (e.g., "Save", "Cancel") do NOT need `aria-label`
- Use descriptive labels: prefer `"Delete conversation"` over `"Delete"`, prefer `"Send message"` over `"Submit"`
- Do not add `aria-label` to non-interactive elements (divs, spans used for layout)

## Files to Modify

- `frontend/src/lib/Sidebar.svelte` — add labels to conversation items, delete buttons
- `frontend/src/lib/Chat.svelte` — add labels to send button, verify input label

## Acceptance Criteria

- [ ] Every button with only an icon has an `aria-label`
- [ ] Conversation list items have descriptive `aria-label` attributes
- [ ] The chat input area has a label (visible or ARIA)
- [ ] No regressions in visual appearance or functionality
- [ ] All existing tests pass

## Test Cases

- [ ] Inspect the Sidebar DOM; assert each conversation item has an `aria-label` attribute
- [ ] Inspect the delete button on a conversation; assert it has `aria-label="Delete conversation"`
- [ ] Inspect the send button in Chat; assert it has an `aria-label`
- [ ] Inspect the message input; assert it has an `aria-label` or associated `<label>`
