# 076 — Fix Gemini tool_result Function Name Hardcode

## Description

In `buddy-core/src/provider/gemini.rs` (line ~85), when sending tool results back to the Gemini API, the function name is hardcoded as `"tool_result"` instead of using the actual function name from the tool call. The Gemini API expects the `functionResponse.name` to match the original `functionCall.name` — using a wrong name likely causes Gemini to reject or misinterpret tool results in multi-tool conversations.

The root cause is that `MessageContent::ToolResult` does not store the tool/function name — only the result content.

## Goal

Tool results sent to the Gemini API include the correct function name that matches the original tool call.

## Requirements

### 1. Extend `MessageContent::ToolResult` to include the tool name

- In `buddy-core/src/types.rs` (or wherever `MessageContent` is defined), add a `name` field to the `ToolResult` variant:
  ```rust
  ToolResult { name: String, content: String }
  ```
- Search for all places that construct `MessageContent::ToolResult` and add the tool name. The tool name is available from the original `ToolCall` that triggered the result — trace where `ToolResult` is created (likely in the tool-loop/chat handler) and pass the name through.

### 2. Use the actual name in the Gemini provider

- In `buddy-core/src/provider/gemini.rs` (line ~85), change:
  ```rust
  "name": "tool_result",
  ```
  to:
  ```rust
  "name": name,
  ```
  where `name` comes from the destructured `MessageContent::ToolResult { name, content }`.

### 3. Update other providers

- Check `openai.rs`, `lmstudio.rs`, `ollama.rs`, and `mistral.rs` for how they handle `ToolResult`. If they also use the tool name (e.g., OpenAI's `tool_call_id`), verify they already have access to it or update them similarly.
- If other providers don't need the name, they can simply ignore it in their destructure: `ToolResult { content, .. }`

### 4. Update serialization

- If `MessageContent` is serialized to the database (via `serde_json` in `store.rs`), ensure the new `name` field is included in serialization. Since it's added to the enum variant, `serde` will pick it up automatically — but verify existing stored conversations can still be deserialized (backward compatibility).
- If old data lacks the `name` field, add `#[serde(default)]` to make it optional during deserialization, defaulting to an empty string.

## Files to Modify

- `buddy-core/src/types.rs` (or wherever `MessageContent` is defined) — add `name` to `ToolResult`
- `buddy-core/src/provider/gemini.rs` — use actual name instead of `"tool_result"`
- Tool-loop handler (likely `buddy-server/src/api/chat.rs` or similar) — pass tool name when constructing `ToolResult`
- Other provider files — update destructure patterns if needed

## Acceptance Criteria

- [ ] `MessageContent::ToolResult` includes a `name: String` field
- [ ] Gemini provider sends the actual function name in `functionResponse.name`
- [ ] All places constructing `ToolResult` provide the tool name
- [ ] Existing stored conversations still deserialize correctly (backward compat)
- [ ] All existing tests pass (`cargo test`)
- [ ] `cargo build --workspace` compiles without new warnings

## Test Cases

- [ ] Construct a `ToolResult` with `name: "get_weather"` and `content: "{...}"`; serialize to Gemini format; assert JSON contains `"name": "get_weather"` (not `"tool_result"`)
- [ ] Deserialize a `ToolResult` JSON blob that lacks the `name` field (old format); assert it deserializes successfully with a default name
- [ ] Run the full tool-loop test with a `SequencedProvider`; assert `ToolResult` messages include the correct tool name
- [ ] Run `cargo test`; assert all existing tests still pass
