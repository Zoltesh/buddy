# 028 — Skill Permission Levels and Approval Flow

## Description

Implement a permission system for skills: each skill declares a `PermissionLevel`, and a middleware layer enforces user approval before executing mutating or network-accessing skills. The UI shows what the skill will do and waits for explicit confirmation.

## Goal

Users maintain control over what buddy does on their behalf. Read-only skills execute freely, but file writes, network requests, and other side-effecting actions require explicit approval.

## Requirements

- Define `PermissionLevel` enum:
  ```rust
  pub enum PermissionLevel {
      ReadOnly,   // No side effects (e.g. read_file, memory_read)
      Mutating,   // Writes to filesystem or state (e.g. write_file, memory_write)
      Network,    // Makes outbound network requests (e.g. fetch_url)
  }
  ```
- Extend the `Skill` trait with a `permission_level(&self) -> PermissionLevel` method
- Assign permission levels to all existing skills:
  - `read_file` → `ReadOnly`
  - `write_file` → `Mutating`
  - `fetch_url` → `Network`
  - `memory_read` → `ReadOnly`
  - `memory_write` → `Mutating`
  - `remember` → `Mutating`
  - `recall` → `ReadOnly`
- Implement an approval flow in the tool-call loop:
  - Before executing a skill, check its permission level
  - If `ReadOnly`, execute immediately
  - If `Mutating` or `Network`, pause and request user approval
  - Emit a new `ChatEvent::ApprovalRequest { id: String, skill_name: String, arguments: serde_json::Value, permission_level: String }` to the frontend via SSE
  - Wait for the user's response via a new `POST /api/chat/{conversation_id}/approve` endpoint
    - Request body: `{ "approval_id": "...", "approved": true | false }`
  - If approved, execute the skill and continue
  - If denied, return a synthetic tool result telling the LLM the action was denied, and continue the loop
  - Timeout: if no approval response within a configurable duration (e.g. 60 seconds), treat as denied
- Add a per-skill approval policy in the config:
  ```toml
  [skills.write_file]
  allowed_directories = ["/home/user/projects"]
  approval = "always"  # "always" | "once" | "trust"
  ```
  - `always` — ask every time (default for `Mutating` and `Network`)
  - `once` — ask once per conversation, then auto-approve for that skill
  - `trust` — never ask (auto-approve)
  - `ReadOnly` skills default to `trust` regardless of config
- The approval mechanism is implemented as middleware in the skill execution path, not inside individual skills

## Acceptance Criteria

- [ ] Every skill declares a `PermissionLevel`
- [ ] `ReadOnly` skills execute without approval
- [ ] `Mutating` skills emit `ApprovalRequest` and wait for user response
- [ ] `Network` skills emit `ApprovalRequest` and wait for user response
- [ ] Approved requests execute the skill and return results to the LLM
- [ ] Denied requests return a "denied" tool result to the LLM (not a crash)
- [ ] Per-skill `approval` config overrides default behavior
- [ ] `trust` policy auto-approves without user interaction
- [ ] `once` policy asks once per conversation then auto-approves
- [ ] Approval timeout treats the request as denied
- [ ] `POST /api/chat/{conversation_id}/approve` endpoint works correctly

## Test Cases

- Execute `read_file` skill; assert it runs without approval request
- Execute `write_file` skill with default config; assert `ApprovalRequest` event is emitted
- Approve a `write_file` request; assert the skill executes and returns results
- Deny a `write_file` request; assert a "denied" tool result is returned to the LLM
- Configure `write_file` with `approval = "trust"`; execute it; assert no approval request
- Configure `write_file` with `approval = "once"`; execute twice in same conversation; assert approval requested only on first call
- Execute a `Network` skill (`fetch_url`); assert `ApprovalRequest` is emitted
- Let an approval request timeout; assert it is treated as denied
- Assert `ApprovalRequest` event includes skill name, arguments, and permission level
- Assert denied tool result message is informative (e.g. "User denied execution of write_file")
