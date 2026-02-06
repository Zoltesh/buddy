# 010 — Tool-Call Loop

## Description

Implement the agentic tool-call loop: the LLM receives skill definitions as tools, can request skill executions via tool calls, the backend executes them and feeds results back, and the LLM continues until it produces a final text response.

## Goal

buddy can autonomously use its skills to accomplish tasks. The LLM decides when and which tools to call, the backend executes them safely, and the conversation continues until the LLM is satisfied.

## Requirements

- Update the `Provider` trait's `complete` method to accept an optional list of tool definitions:
  ```rust
  async fn complete(&self, messages: Vec<Message>, tools: Option<Vec<serde_json::Value>>) -> Result<TokenStream, ProviderError>
  ```
- Update `Token` (or introduce a new enum) to represent both text deltas and tool-call requests:
  - `Token::Text { text: String }` — streamed text content
  - `Token::ToolCall { id: String, name: String, arguments: String }` — the LLM is requesting a tool execution
- Update `OpenAiProvider` to:
  - Include `tools` in the request body when provided
  - Parse `tool_calls` from the streamed response (accumulate across chunks)
  - Yield `Token::ToolCall` when a complete tool call is assembled
- Implement the tool-call loop in the API layer (`chat_handler` or a new orchestrator):
  1. Send messages + tool definitions to the provider
  2. If the provider returns a tool call: execute the skill via `SkillRegistry`, append the `ToolCall` and `ToolResult` messages to the conversation, and call the provider again
  3. Repeat until the provider returns a text response (no more tool calls)
  4. Stream the final text response to the client
- Tool calls and results are included in the SSE stream so the frontend can render them:
  - New `ChatEvent` variants: `ToolCallStart { id, name, arguments }`, `ToolCallResult { id, content }`
- The loop has a maximum iteration limit (e.g., 10) to prevent runaway tool-call chains
- Errors during skill execution are fed back to the LLM as error tool results (not fatal to the conversation)

## Acceptance Criteria

- [x] The LLM receives tool definitions and can decide to call a tool
- [x] Tool calls are executed via the `SkillRegistry` and results fed back to the LLM
- [x] The LLM can chain multiple tool calls before producing a final response
- [x] Tool-call events are streamed to the client via SSE
- [x] A maximum iteration limit prevents infinite tool-call loops
- [x] Skill execution errors are returned to the LLM as error results, not crashes
- [x] The `Provider` trait change is backward-compatible (passing `None` for tools works as before)
- [x] Existing v0.1 chat functionality (no tools) continues to work unchanged

## Test Cases

- Send a prompt that triggers a tool call (e.g., "Read the file at /sandbox/test.txt"); assert the backend executes `read_file` and the LLM incorporates the result
- Mock a provider that returns a tool call; assert the orchestrator executes the skill and calls the provider again with the result
- Mock a provider that chains 3 tool calls; assert all 3 are executed and the final text response is streamed
- Mock a provider that would chain 11+ tool calls; assert the loop stops at the limit and returns an error
- Mock a skill that returns `SkillError`; assert the error is fed back to the LLM as a tool result (not a crash)
- Send a normal chat message (no tool triggers); assert behavior is identical to v0.1
- Assert SSE stream contains `ToolCallStart` and `ToolCallResult` events for each tool call
