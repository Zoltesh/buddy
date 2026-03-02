# 079 — Standardize Reactive Array Mutations in Chat.svelte

## Description

`frontend/src/lib/Chat.svelte` directly mutates the `displayItems` reactive array using `.push()`, `.splice()`, and `.pop()`. While Svelte 5's `$state` proxy does track these mutations, the rest of the codebase uses immutable reassignment patterns (e.g., `items = [...items, newItem]`). This inconsistency makes the reactivity model harder to reason about.

## Goal

All mutations to `displayItems` in `Chat.svelte` use immutable reassignment patterns consistent with the rest of the codebase.

## Requirements

- In `frontend/src/lib/Chat.svelte`, find all direct mutations on `displayItems` and replace them with immutable reassignments:

  | Line (approx) | Current | Replacement |
  |---|---|---|
  | 98 | `displayItems.push(userItem)` | `displayItems = [...displayItems, userItem]` |
  | 115 | `displayItems.push({ kind: 'text', ... })` | `displayItems = [...displayItems, { kind: 'text', ... }]` |
  | 168 | `displayItems.splice(currentAssistantIdx, 1)` | `displayItems = displayItems.filter((_, i) => i !== currentAssistantIdx)` |
  | 170 | `displayItems.push({ kind: 'tool_call', ... })` | `displayItems = [...displayItems, { kind: 'tool_call', ... }]` |
  | 185 | `displayItems.push({ kind: 'text', ... })` | `displayItems = [...displayItems, { kind: 'text', ... }]` |
  | 201 | `displayItems.pop()` | `displayItems = displayItems.slice(0, -1)` |

- Do NOT change the logic — only the mutation style. The values being pushed/spliced/popped must remain identical.
- Verify that `displayItems` is declared with `let displayItems = $state([])` — if so, reassignment triggers reactivity correctly.
- Check for any other direct mutations on reactive arrays in this file that aren't listed above.

## Files to Modify

- `frontend/src/lib/Chat.svelte` — replace 6 direct mutations with immutable patterns

## Acceptance Criteria

- [ ] No `.push()`, `.pop()`, `.splice()`, or `.shift()` calls on `displayItems`
- [ ] All replacements use spread/filter/slice patterns
- [ ] Chat functionality is unchanged — messages appear, stream, and display identically
- [ ] All existing tests pass

## Test Cases

- [ ] Send a message in the chat; assert the user message and assistant response both appear in the display
- [ ] Verify a streaming response with tool calls renders correctly (tool call items appear, then final text)
- [ ] Run any existing Chat.svelte tests; assert they pass
