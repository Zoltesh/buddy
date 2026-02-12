# 058 — Telegram Skill Approval Flow

## Description

When buddy wants to use a skill (tool) during a Telegram conversation, the user must be able to approve or deny the action — just like the web UI's approval flow. This task implements skill execution over Telegram using inline keyboard buttons for approval. When a skill requires permission, buddy sends a message describing the action with Approve/Deny buttons. The user taps a button, and buddy proceeds accordingly. A configurable timeout defaults to deny if the user doesn't respond.

## Goal

Users can approve or deny skill execution during Telegram conversations using inline keyboard buttons. Skills execute and return results that are included in the conversation.

## Requirements

- When the provider requests a tool call during a Telegram conversation:
  1. Look up the skill in the `SkillRegistry`
  2. Check the skill's `PermissionLevel`:
     - `ReadOnly`: execute immediately (no approval needed)
     - `Mutating` or `Network`: check the approval policy (same logic as web)
       - `Trust`: execute immediately
       - `Once`: check if already approved for this conversation, if not → ask
       - `Always`: always ask
  3. If approval is needed, send a Telegram message with:
     - Text: "🔧 **{skill_name}** wants to execute:\n```\n{formatted_arguments}\n```\nAllow this action?"
     - Inline keyboard with two buttons: "✅ Approve" and "❌ Deny"
  4. Wait for the user's button press (callback query):
     - Approve: execute the skill, edit the original message to show "✅ Approved", send the result
     - Deny: edit the original message to show "❌ Denied", send a tool result indicating denial
     - Timeout (configurable, default 60 seconds): treat as deny, edit message to show "⏰ Timed out (denied)"
  5. After execution, append `ToolCall` and `ToolResult` messages to the conversation
  6. Call the provider again with the updated messages (tool loop, same as web)
- **Tool loop limit:** maximum 10 iterations (same as web), to prevent runaway tool execution
- **Callback query handling:**
  - Use teloxide's callback query handler
  - Match callback data to pending approvals (store pending approvals in an in-memory map keyed by message_id)
  - Answer the callback query to dismiss the Telegram loading indicator
- **Result formatting:**
  - Tool results are formatted as code blocks in the Telegram message
  - If the result is too long (> 2000 chars), truncate with "... (truncated)"
  - Errors are shown as: "❌ Tool error: {message}"
- Reuse the `PendingApprovals` pattern from buddy-core's `AppState` if possible, or implement a Telegram-specific equivalent

## Acceptance Criteria

- [x] ReadOnly skills execute immediately without asking for approval
- [x] Mutating/Network skills with `Always` policy show approval buttons
- [x] Mutating/Network skills with `Trust` policy execute immediately
- [x] Mutating/Network skills with `Once` policy ask once per conversation, then auto-approve
- [x] Inline keyboard with Approve/Deny buttons is sent for skills requiring approval
- [x] Tapping Approve executes the skill and shows the result
- [x] Tapping Deny sends a denial tool result and continues the conversation
- [x] Timeout (60s default) treats the action as denied
- [x] Tool call and result messages are stored in the conversation database
- [x] Tool loop continues after skill execution (provider called again with updated messages)
- [x] Tool loop stops after 10 iterations maximum
- [x] All existing tests pass

## Test Cases

- [x] Send a message that triggers a ReadOnly skill (e.g., recall); assert the skill executes without showing approval buttons
- [x] Send a message that triggers a Mutating skill with `Always` policy; assert an inline keyboard message is sent with Approve and Deny buttons
- [x] Tap "Approve" on a skill approval message; assert the skill executes, the original message is edited to show "Approved", and the result is sent
- [x] Tap "Deny" on a skill approval message; assert the original message is edited to show "Denied" and a denial tool result is sent to the provider
- [x] Do not respond to a skill approval message within the timeout; assert the message is edited to show "Timed out (denied)" and a denial result is sent
- [x] Send a message that triggers multiple consecutive tool calls; assert each one goes through the approval flow and the tool loop continues up to 10 iterations
- [x] After a skill approval and execution, assert the ToolCall and ToolResult messages are stored in the conversation database
- [x] Send a message that triggers a Network skill with `Trust` policy; assert it executes immediately without approval buttons
