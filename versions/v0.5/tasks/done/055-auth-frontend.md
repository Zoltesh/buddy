# 055 — Authentication Frontend

## Description

When the backend requires authentication (task 054), the frontend needs a login gate. This task adds a simple login page that appears before the main app when auth is required. The user enters their token, the frontend verifies it, and stores it for subsequent API calls. This is deliberately minimal — no session management, no password change UI, no "remember me" beyond localStorage.

## Goal

Users are prompted for their token when auth is required and can use buddy normally after authenticating. The token persists across page reloads via localStorage.

## Requirements

- On app startup, call `GET /api/auth/status`
  - If `required: false`: proceed to main app as normal
  - If `required: true`: check localStorage for a stored token
    - If token exists: verify it via `POST /api/auth/verify`
      - If valid: proceed to main app
      - If invalid: clear stored token, show login page
    - If no token: show login page
- **Login page:**
  - Centered card with buddy branding
  - Single text input field labeled "Access Token" (type: password, to mask input)
  - "Sign In" button
  - On submit: call `POST /api/auth/verify` with the entered token
    - If valid: store token in localStorage under key `buddy_auth_token`, proceed to main app
    - If invalid: show inline error "Invalid token. Please try again." Clear the input
  - No username field — this is single-user token auth
- **API calls:**
  - Modify the `api.js` module: all `fetch()` calls include `Authorization: Bearer <token>` header when a token is stored in localStorage
  - Create a wrapper function or modify existing helpers to inject the header automatically
  - If any API call returns 401: clear the stored token and redirect to login page
- **Sign out:**
  - Add a "Sign Out" option in the sidebar (only visible when auth is enabled)
  - On click: clear localStorage token, redirect to login page
- **No backend changes.** This is a frontend-only task. It depends on task 054 being complete.

## Acceptance Criteria

- [x] App checks `GET /api/auth/status` on startup
- [x] Login page appears when auth is required and no valid token is stored
- [x] Login page has a token input field and "Sign In" button
- [x] Valid token submission stores the token and shows the main app
- [x] Invalid token submission shows an error message
- [x] Stored token is included as a Bearer token in all API requests
- [x] A 401 response from any API call clears the token and shows the login page
- [x] "Sign Out" button in sidebar clears the token and shows the login page
- [x] When auth is not required, the app loads normally with no login page
- [x] Token persists across page reloads via localStorage
- [x] All existing functionality works after authentication

## Test Cases

- [x] Load the app when `GET /api/auth/status` returns `{ "required": false }`; main app renders immediately with no login page
- [x] Load the app when auth is required and no token is in localStorage; login page renders with token input and "Sign In" button
- [x] Enter a valid token on the login page and click "Sign In"; login page disappears, main app renders, token is in localStorage
- [x] Enter an invalid token on the login page and click "Sign In"; error message "Invalid token. Please try again." appears, input is cleared
- [x] Reload the page after authenticating; app checks stored token validity and renders the main app without showing login page
- [x] Clear the token manually from localStorage and reload; login page appears
- [x] While authenticated, make an API call that returns 401 (e.g., token was invalidated server-side); login page appears
- [x] Click "Sign Out" in the sidebar; localStorage token is cleared, login page appears
- [x] When auth is required, verify that API calls (e.g., fetching conversations) include the `Authorization: Bearer <token>` header
- [x] When auth is not required, verify that no "Sign Out" button appears in the sidebar
