# 023 — Short-Term Working Memory Skills

## Description

Implement a structured scratchpad that persists within a conversation, accessible to the LLM via `memory_read` and `memory_write` skills. The scratchpad supports key-value pairs and free-form notes. It is cleared when the conversation ends.

## Goal

The LLM can maintain working context within a conversation — tracking names, preferences, intermediate results, and other state it decides is worth remembering — without relying on the full message history.

## Requirements

- Define a `WorkingMemory` struct:
  - Key-value store: `HashMap<String, String>` for structured facts (e.g. `"user_name" -> "Alice"`)
  - Notes: `Vec<String>` for free-form observations
  - Methods: `set(key, value)`, `get(key) -> Option<&str>`, `delete(key)`, `list_keys() -> Vec<&str>`, `add_note(text)`, `get_notes() -> &[String]`, `clear()`
  - Serializable to JSON for inclusion in LLM context
- `WorkingMemory` is stored per-conversation in memory (not persisted to database)
  - Use a `HashMap<String, WorkingMemory>` keyed by conversation ID, held in `AppState`
  - Cleaned up when a conversation is deleted or when buddy restarts
- Implement `memory_write` skill:
  - Input schema: `{ "action": "set" | "note" | "delete" | "clear", "key": "...", "value": "..." }`
  - `set` requires `key` and `value`; `note` requires `value`; `delete` requires `key`; `clear` requires nothing
  - Returns confirmation of what was written/deleted
- Implement `memory_read` skill:
  - Input schema: `{ "key": "..." }` (optional)
  - If `key` is provided, return that key's value (or "not found")
  - If `key` is omitted, return the full scratchpad contents (all key-value pairs and notes)
- Register both skills in the `SkillRegistry`
- The working memory contents are included in the system prompt sent to the LLM so it can see its own notes
  - Injected as a section in the system message when the scratchpad is non-empty

## Acceptance Criteria

- [x] `memory_write` with `action: "set"` stores a key-value pair retrievable by `memory_read`
- [x] `memory_write` with `action: "note"` appends a free-form note
- [x] `memory_write` with `action: "delete"` removes a key
- [x] `memory_write` with `action: "clear"` empties the entire scratchpad
- [x] `memory_read` with a key returns that key's value
- [x] `memory_read` without a key returns all stored data
- [x] Working memory is per-conversation (two conversations have independent scratchpads)
- [x] Working memory is cleared on server restart (not persisted)
- [x] Non-empty working memory is included in the system prompt context
- [x] Both skills appear in `tool_definitions()` with correct schemas

## Test Cases

- [x] Call `memory_write` with `{ "action": "set", "key": "name", "value": "Alice" }`; call `memory_read` with `{ "key": "name" }`; assert result contains `"Alice"`
- [x] Call `memory_write` with `{ "action": "note", "value": "User prefers dark mode" }`; call `memory_read` with no key; assert notes contain the text
- [x] Call `memory_write` with `{ "action": "delete", "key": "name" }` after setting it; call `memory_read` with `{ "key": "name" }`; assert not found
- [x] Call `memory_write` with `{ "action": "clear" }` after setting several entries; call `memory_read`; assert empty
- [x] Write to conversation "A"; read from conversation "B"; assert B's scratchpad is empty
- [x] Call `memory_write` with invalid action (e.g. `"action": "foo"`); assert `SkillError::InvalidInput`
- [x] Call `memory_write` with `"set"` but missing `key`; assert `SkillError::InvalidInput`
