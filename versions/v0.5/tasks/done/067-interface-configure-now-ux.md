# 067 — Interface Configure-Now UX

## Description

The Interfaces page currently shows "Add `[interfaces.telegram]` to your buddy.toml to enable" for unconfigured interfaces. This requires the user to leave the UI, find the config file, and hand-edit TOML. The UI should be the primary configuration surface — the user clicks "Configure", fills in the fields, and saves. The config is written to `buddy.toml` behind the scenes via the existing `PUT /api/config/interfaces` endpoint. The same treatment applies to both Telegram and WhatsApp cards.

## Goal

Users can configure any interface entirely from the Interfaces page without ever editing `buddy.toml` by hand.

## Requirements

- **Replace empty-state text with a "Configure" button:**
  - Remove the "Add `[interfaces.telegram]` to your buddy.toml to enable" and "Add `[interfaces.whatsapp]` to your buddy.toml to enable" messages
  - Replace each with a brief description of the interface and a "Configure" button
  - Telegram description: "Connect buddy to a Telegram bot for chat via Telegram."
  - WhatsApp description: "Connect buddy to WhatsApp Business for chat via WhatsApp."
- **"Configure" opens the inline edit form:**
  - Clicking "Configure" opens the same inline editing form used by the existing "Edit" button
  - For Telegram: pre-fill `bot_token_env` with the default (`TELEGRAM_BOT_TOKEN`), `enabled` unchecked
  - For WhatsApp: pre-fill all fields with their defaults, `enabled` unchecked
  - The form includes Save and Cancel buttons (reuse existing `saveEditing()` / `cancelEditing()` logic)
- **Save writes to `buddy.toml`:**
  - Saving calls `PUT /api/config/interfaces` with the updated section, which writes to `buddy.toml` via the existing `apply_config_update` mechanism
  - After saving, the card transitions to the configured state showing the config summary, toggle, and Edit button
- **Cancel discards changes:**
  - Clicking Cancel closes the form and returns to the unconfigured empty state
- **No backend changes required** — the existing `PUT /api/config/interfaces` endpoint already handles creating/updating the full interfaces config section
- **Frontend-only change** in `Interfaces.svelte`

## Acceptance Criteria

- [x] Unconfigured Telegram card shows a description and "Configure" button instead of the buddy.toml message
- [x] Unconfigured WhatsApp card shows a description and "Configure" button instead of the buddy.toml message
- [x] Clicking "Configure" opens the inline edit form pre-filled with defaults
- [x] Saving the form writes the config to `buddy.toml` and the card shows the configured state
- [x] Cancelling the form returns to the unconfigured state without changes
- [x] Already-configured interfaces continue to show the existing config summary, toggle, and Edit button
- [x] All existing functionality (edit, toggle, save) continues to work

## Test Cases

- [x] Load Interfaces page with no Telegram config; assert Telegram card shows description and "Configure" button (no buddy.toml reference)
- [x] Load Interfaces page with no WhatsApp config; assert WhatsApp card shows description and "Configure" button (no buddy.toml reference)
- [x] Click "Configure" on Telegram card; assert inline form appears with `bot_token_env` pre-filled as `TELEGRAM_BOT_TOKEN`
- [x] Click "Configure" on WhatsApp card; assert inline form appears with defaults (api_token_env, phone_number_id, verify_token, webhook_port)
- [x] Fill in Telegram form and click Save; assert `PUT /api/config/interfaces` is called and the card transitions to configured state
- [x] Click "Configure" then click Cancel; assert the card returns to the unconfigured state and no API call is made
- [x] Load page with Telegram already configured; assert the card shows config summary, toggle, and Edit button (not the Configure button)
