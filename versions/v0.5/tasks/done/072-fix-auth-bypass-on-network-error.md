# 072 — Fix Auth Bypass on Network Error

## Description

In `frontend/src/App.svelte` (lines 86-92), when the `GET /api/auth/status` call fails (network error, server unreachable, timeout), the `catch` block sets `authRequired = false` and `authenticated = true`. This means a network hiccup causes the app to load as if no authentication is configured, bypassing the login screen entirely.

## Goal

When the auth status check fails, the app shows an error state instead of granting unauthenticated access.

## Requirements

- In `frontend/src/App.svelte`, change the `catch` block of the auth status check (around line 86) to:
  - Set `authRequired = true` (assume auth is needed when we can't confirm)
  - Set `authenticated = false`
  - Set `authChecking = false`
  - Store an error message in a new reactive variable (e.g., `let authError = $state('')`) with text like `"Unable to connect to server. Please check your connection and reload."`
  - Do NOT call `loadConversations()`
- In the template section of `App.svelte`, add an error state that renders when `authError` is truthy:
  - Show the error message
  - Include a "Retry" button that re-runs the auth status check
  - This should appear instead of the login form or the main app
- Keep the existing successful auth flow unchanged

## Files to Modify

- `frontend/src/App.svelte` — modify catch block, add error state variable, add error UI

## Acceptance Criteria

- [x] Network failure during auth check does NOT set `authenticated = true`
- [x] Network failure shows an error message to the user
- [x] Error state includes a Retry button that re-checks auth status
- [x] Successful auth check still works as before (no regression)
- [x] When server comes back online, clicking Retry recovers to normal login flow
- [x] All existing tests pass

## Test Cases

- [x] Mock `fetch('/api/auth/status')` to throw a network error; assert `authenticated` is `false` and `authRequired` is `true`
- [x] Mock `fetch('/api/auth/status')` to throw; assert `authError` contains an error message string
- [x] Mock `fetch('/api/auth/status')` to return `{ required: true }`; assert normal login flow is shown (no regression)
- [x] Mock `fetch('/api/auth/status')` to return `{ required: false }`; assert app loads normally (no regression)
