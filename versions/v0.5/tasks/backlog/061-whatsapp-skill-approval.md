# 061 — WhatsApp Skill Approval Flow

## Description

When buddy wants to use a skill during a WhatsApp conversation, the user must approve or deny. Unlike Telegram's inline keyboard buttons, WhatsApp uses interactive messages with quick reply buttons. This task implements skill execution over WhatsApp with approval via quick reply buttons, following the same permission model as the web and Telegram interfaces.

## Goal

Users can approve or deny skill execution during WhatsApp conversations using quick reply buttons. Skills execute and return results that are included in the conversation.

## Requirements

- When the provider requests a tool call during a WhatsApp conversation:
  1. Look up the skill and check `PermissionLevel` (same logic as web and Telegram)
  2. `ReadOnly` or `Trust` policy: execute immediately
  3. If approval needed, send an interactive message via WhatsApp API:
     - Type: `interactive` with `type: "button"`
     - Body: "🔧 {skill_name} wants to execute:\n{formatted_arguments}\n\nAllow this action?"
     - Buttons: `[{ id: "approve", title: "Approve" }, { id: "deny", title: "Deny" }]`
  4. Wait for the user's button reply:
     - "approve": execute skill, send result
     - "deny": send denial result to provider
     - Timeout (60 seconds): treat as deny
  5. Append ToolCall and ToolResult messages to conversation
  6. Continue the tool loop (up to 10 iterations)
- **Interactive message API:**
  - POST to `https://graph.facebook.com/v21.0/{phone_number_id}/messages`
  - Body includes `type: "interactive"` with button definitions
  - Add `send_interactive_message()` to `WhatsAppClient`
- **Button reply handling:**
  - WhatsApp button replies come as webhook events with `type: "interactive"` and `button_reply.id`
  - Match to pending approvals via conversation context
  - Store pending approvals in an in-memory map keyed by sender phone number (one pending approval per sender at a time)
- **Result formatting:**
  - Tool results sent as text messages
  - Truncate results longer than 2000 characters
  - Errors shown as: "Tool error: {message}"
- Timeout and tool loop limits match Telegram (task 058)

## Acceptance Criteria

- [ ] ReadOnly skills execute immediately without approval
- [ ] Mutating/Network skills with appropriate policies show approval buttons
- [ ] Interactive message with Approve/Deny buttons is sent via WhatsApp API
- [ ] Tapping Approve executes the skill and returns the result
- [ ] Tapping Deny sends a denial to the provider
- [ ] Timeout (60s) defaults to deny
- [ ] Tool call and result messages are stored in the database
- [ ] Tool loop continues after execution (up to 10 iterations)
- [ ] `WhatsAppClient` has a `send_interactive_message()` method
- [ ] All existing tests pass

## Test Cases

- [ ] Trigger a ReadOnly skill via WhatsApp; assert it executes without interactive buttons
- [ ] Trigger a Mutating skill with `Always` policy; assert an interactive button message is sent
- [ ] Send an "approve" button reply; assert the skill executes and the result is sent
- [ ] Send a "deny" button reply; assert a denial result is sent to the provider
- [ ] Do not respond within timeout; assert the action is denied and the user is notified
- [ ] Trigger multiple consecutive tool calls; assert each goes through the approval flow
- [ ] Assert ToolCall and ToolResult messages are stored in the conversation database
