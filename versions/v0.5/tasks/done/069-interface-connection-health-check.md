# 069 — Interface Connection Health Check

## Description

The Interfaces page currently shows "Connected" for any enabled interface, but it doesn't actually verify the connection. This task adds a server-side health check that validates configured credentials against the external API (Telegram's `getMe`, WhatsApp's Graph API). The frontend uses this to show accurate connection status: valid token, invalid token, or unreachable.

## Goal

Users can see whether their configured interface credentials are actually valid and working, directly from the Interfaces page.

## Requirements

- **New API endpoint:** `POST /api/interfaces/check`
  - Request body: `{ "interface": "telegram" }` or `{ "interface": "whatsapp" }`
  - Resolves the token/credentials from config (using `resolve_bot_token()` for Telegram, env var for WhatsApp)
  - **Telegram check:** sends `GET https://api.telegram.org/bot<token>/getMe`
    - Success (200 + `ok: true`): return `{ "status": "connected", "detail": "Bot: @bot_username" }`
    - Auth failure (401 or `ok: false`): return `{ "status": "error", "detail": "Invalid bot token" }`
    - Network error: return `{ "status": "error", "detail": "Could not reach Telegram API" }`
  - **WhatsApp check:** sends `GET https://graph.facebook.com/v22.0/<phone_number_id>` with Bearer token
    - Success: return `{ "status": "connected", "detail": "Phone: <phone_number_id>" }`
    - Auth failure: return `{ "status": "error", "detail": "Invalid API token" }`
    - Network error: return `{ "status": "error", "detail": "Could not reach WhatsApp API" }`
  - If the interface is not configured or token cannot be resolved: return `{ "status": "error", "detail": "Not configured" }`
  - This endpoint should have a reasonable timeout (5 seconds) to avoid hanging the UI
- **Frontend changes:**
  - Add a "Check Connection" button on each configured interface card
  - Clicking it calls `POST /api/interfaces/check` for that interface
  - While checking: show a spinner and "Checking..." text
  - On success: show green "Connected" with bot username / phone ID detail
  - On error: show red/amber status with the error detail
  - Auto-check on page load for enabled interfaces (with a small delay to avoid blocking render)
  - The status indicator updates to reflect the check result:
    - Green dot + "Connected — @bot_username" for valid Telegram
    - Red dot + "Invalid bot token" or "Could not reach API" for errors
    - Gray dot + "Disabled" when `enabled: false` (no check performed)
    - Gray dot + "Not configured" when unconfigured (no check performed)
- **Frontend API addition** (`api.js`):
  - Add `checkInterfaceConnection(name)` function that calls `POST /api/interfaces/check`
- **Backend route registration:**
  - Add `POST /api/interfaces/check` to the protected routes in `buddy-server/src/main.rs`

## Acceptance Criteria

- [x] `POST /api/interfaces/check` with `"telegram"` validates the bot token against Telegram's API
- [x] `POST /api/interfaces/check` with `"whatsapp"` validates the API token against WhatsApp's Graph API
- [x] Valid Telegram token returns `status: "connected"` with bot username
- [x] Invalid Telegram token returns `status: "error"` with descriptive message
- [x] Network failure returns `status: "error"` with connectivity message
- [x] Unconfigured interface returns `status: "error"` with "Not configured"
- [x] Frontend shows "Check Connection" button on configured cards
- [x] Frontend shows spinner during check and result afterward
- [x] Enabled interfaces are auto-checked on page load
- [x] Disabled interfaces show "Disabled" without performing a check
- [x] Health check has a 5-second timeout
- [x] All existing tests pass

## Test Cases

- [x] Call `POST /api/interfaces/check` with `{"interface": "telegram"}` when Telegram is not configured; assert response is `{ "status": "error", "detail": "Not configured" }`
- [x] Call `POST /api/interfaces/check` with `{"interface": "whatsapp"}` when WhatsApp is not configured; assert response is `{ "status": "error", "detail": "Not configured" }`
- [x] Call `POST /api/interfaces/check` with `{"interface": "telegram"}` when Telegram is configured but token resolution fails (env var not set, no direct token); assert response is `{ "status": "error" }` with detail mentioning the missing token
- [x] Call `POST /api/interfaces/check` with `{"interface": "unknown"}`; assert 400 error response
- [x] Call `POST /api/interfaces/check` with valid Telegram token (integration test, `#[ignore]`); assert response is `{ "status": "connected" }` with bot username in detail
- [x] Call `POST /api/interfaces/check` with invalid Telegram token (integration test, `#[ignore]`); assert response is `{ "status": "error" }` with "Invalid bot token" in detail
- [ ] Verify the check endpoint has a timeout: mock a slow-responding server; assert the endpoint returns within 6 seconds with an error
