# Add Tool Approval Request UI

## Description

The backend sends an `ApprovalRequest` event when a non-ReadOnly tool (Network, Mutating permission) is called, but the frontend doesn't handle this event. This causes the approval to time out after 60 seconds, returning "User denied execution" even though the user was never prompted.

## Root Cause

- Backend: Sends `ChatEvent::ApprovalRequest` correctly (chat.rs:234-241)
- Frontend: Only handles `tool_call_start` and `tool_call_result` events in Chat.svelte
- Missing: No approval dialog UI component to display and handle user approval/denial

## Files Modified

- `frontend/src/lib/Chat.svelte` — Handle `ApprovalRequest` event type
- `frontend/src/lib/api.js` — Added `approveTool` function
- Created `frontend/src/lib/ApprovalDialog.svelte` — New component for approval UI

## Dependencies

- None — this is a standalone fix

## Acceptance Criteria

- [x] Frontend displays approval dialog when Network/Mutating tool is called
- [x] Dialog shows tool name, arguments, and permission level
- [x] User can approve or deny the tool execution
- [x] Approval/denial is sent back to backend
- [x] Backend receives the response and proceeds accordingly

## Test Cases

- [ ] Call fetch_url tool and verify approval dialog appears
- [ ] Approve the request and verify tool executes successfully
- [ ] Deny the request and verify "User denied" message appears
- [ ] Verify timeout behavior (if user doesn't respond in 60s)
