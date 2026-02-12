# 064 — Interface Management UI

## Description

With the Interfaces route established (task 063), this task builds the actual management UI: cards for each configured interface (Telegram, WhatsApp) showing connection status, configuration summary, and controls. The UI reads interface config from the backend and displays it in a clear, actionable layout. This is the web-based control panel for buddy's multi-interface capability.

## Goal

Users can see the status of all configured interfaces, view their configuration, and enable/disable them from the Interfaces page.

## Requirements

- **Interface cards:** For each configured interface, display a card with:
  - **Header:** Interface name (e.g., "Telegram", "WhatsApp") with an icon
  - **Status indicator:**
    - Green dot + "Connected" when the interface binary is running and connected
    - Red dot + "Disconnected" when not running or unreachable
    - Gray dot + "Disabled" when `enabled: false` in config
  - **Configuration summary:**
    - Telegram: bot token env var name (not the value), enabled status
    - WhatsApp: phone number ID, webhook port, enabled status
  - **Enable/Disable toggle:** switches the `enabled` field in config via `PUT /api/config/interfaces`
  - **Edit button:** opens inline editing for the interface config fields (same pattern as provider editing in ModelsTab)
  - **Save button:** persists changes via `PUT /api/config/interfaces`
- **Backend API addition:**
  - Add `PUT /api/config/interfaces` endpoint in `buddy-server` to update the `[interfaces]` section of config
  - Add `GET /api/interfaces/status` endpoint that returns the connection status of each interface
    - For now, this checks if the config is present and enabled — actual live connection status requires inter-process communication which is deferred
    - Response: `{ "telegram": { "configured": true, "enabled": true }, "whatsapp": { "configured": false, "enabled": false } }`
- **Empty state handling:**
  - If an interface is not configured at all, show a muted card with "Not configured" and a help text: "Add [interfaces.telegram] to your buddy.toml to enable."
- **No interface start/stop from UI** — the interface binaries are managed externally (systemd, Docker, etc.). The UI only manages config and shows status.
- Fetch config on mount to populate the interface cards

## Acceptance Criteria

- [x] Telegram card displays when Telegram is configured in config
- [x] WhatsApp card displays when WhatsApp is configured in config
- [x] Each card shows the correct status indicator (connected/disconnected/disabled)
- [x] Enable/disable toggle updates the config via `PUT /api/config/interfaces`
- [x] Inline editing allows changing interface config fields
- [x] Save persists changes to `buddy.toml` via the API
- [x] Unconfigured interfaces show a "Not configured" card with help text
- [x] `PUT /api/config/interfaces` endpoint validates and saves interface config
- [x] `GET /api/interfaces/status` returns configuration and enabled status
- [x] All existing functionality works

## Test Cases

- [x] Load Interfaces page with Telegram configured and enabled; card shows "Telegram" with green "Connected" indicator (if enabled) or status
- [x] Load Interfaces page with WhatsApp not configured; card shows "Not configured" with help text
- [x] Toggle Telegram from enabled to disabled; assert `PUT /api/config/interfaces` is called and the toggle reflects the new state
- [x] Edit the Telegram `bot_token_env` field and click Save; assert the config is updated
- [x] Load Interfaces page with both interfaces configured; both cards are visible
- [x] Call `GET /api/interfaces/status` with Telegram configured and enabled; assert `{ "telegram": { "configured": true, "enabled": true } }`
- [x] Call `GET /api/interfaces/status` with nothing configured; assert both interfaces show `configured: false`
- [x] Call `PUT /api/config/interfaces` with invalid data; assert validation error
