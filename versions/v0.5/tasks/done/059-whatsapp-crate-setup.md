# 059 — WhatsApp Bot Crate Setup

## Description

This task creates the `buddy-whatsapp` binary crate — a WhatsApp Business Cloud API client that consumes `buddy-core`. It establishes the crate structure, config integration, message adapter, and a webhook receiver for incoming messages. The WhatsApp Business API works differently from Telegram: instead of polling, WhatsApp sends webhooks to a URL you control, and you reply via REST API calls. This task sets up the infrastructure; conversation flow comes in the next task.

## Goal

A `buddy-whatsapp` binary starts an HTTP server that receives WhatsApp webhook events, validates them, and logs incoming messages. The message adapter can translate between WhatsApp and buddy-core message formats.

## Requirements

- Create `buddy-whatsapp/` as a new workspace member
- `buddy-whatsapp/Cargo.toml`:
  - Depends on `buddy-core` (path dependency)
  - Depends on `axum` (for the webhook receiver — this is a separate Axum instance from buddy-server)
  - Depends on `reqwest` (for sending messages via the WhatsApp API)
  - Depends on `tokio`, `serde`, `serde_json`, `log`, `env_logger`
- Add `"buddy-whatsapp"` to workspace members in root `Cargo.toml`
- Add `[interfaces.whatsapp]` config section in `buddy-core/src/config.rs`:
  ```toml
  [interfaces.whatsapp]
  enabled = false
  api_token_env = "WHATSAPP_API_TOKEN"
  phone_number_id = ""
  verify_token = ""
  webhook_port = 8444
  ```
  - `enabled`: boolean, default `false`
  - `api_token_env`: env var name for the WhatsApp Business API access token
  - `phone_number_id`: the WhatsApp Business phone number ID
  - `verify_token`: token used for webhook verification handshake
  - `webhook_port`: port for the webhook receiver (default: 8444)
  - Add `WhatsAppConfig` struct to config
  - Add `whatsapp: WhatsAppConfig` to `InterfacesConfig`
- Create `buddy-whatsapp/src/main.rs`:
  - Load config from `buddy.toml`
  - Start an Axum server on the configured `webhook_port`
  - Implement `GET /webhook` for Meta's verification handshake:
    - Check `hub.mode == "subscribe"` and `hub.verify_token` matches config
    - Return `hub.challenge` as plain text
  - Implement `POST /webhook` for incoming messages:
    - Parse the webhook payload (Meta's webhook format)
    - Extract text messages, log sender and content
    - Return 200 OK immediately (WhatsApp requires fast responses)
  - Graceful shutdown on SIGINT/SIGTERM
- Create `buddy-whatsapp/src/adapter.rs`:
  - `fn whatsapp_to_buddy(message: &WhatsAppMessage) -> Option<buddy_core::types::Message>` — extracts text from a WhatsApp message payload and converts to buddy-core `Message`
  - `fn buddy_to_whatsapp(message: &buddy_core::types::Message) -> String` — converts buddy-core `Message` to plain text suitable for WhatsApp
  - Define `WhatsAppMessage` struct that models the relevant fields from Meta's webhook payload
- Create `buddy-whatsapp/src/client.rs`:
  - `WhatsAppClient` struct for sending messages via the WhatsApp Business API
  - `async fn send_text_message(&self, to: &str, text: &str) -> Result<(), WhatsAppError>`
  - API call: `POST https://graph.facebook.com/v22.0/{phone_number_id}/messages`
  - Headers: `Authorization: Bearer {token}`, `Content-Type: application/json`
  - Body: `{ "messaging_product": "whatsapp", "to": "{recipient}", "text": { "body": "{text}" } }`
- `cargo build -p buddy-whatsapp` must succeed

## Acceptance Criteria

- [x] `buddy-whatsapp/` exists as a workspace member
- [x] `buddy-whatsapp/Cargo.toml` depends on `buddy-core`, `axum`, and `reqwest`
- [x] `[interfaces.whatsapp]` config section parses correctly with all fields
- [x] Config without `[interfaces.whatsapp]` parses correctly (defaults to disabled)
- [x] Webhook verification handshake (`GET /webhook`) returns the challenge when tokens match
- [x] Webhook verification rejects requests with wrong verify token (403)
- [x] Incoming messages (`POST /webhook`) are parsed and logged
- [x] `WhatsAppClient` can send text messages via the API
- [x] Message adapter converts between WhatsApp and buddy-core formats
- [x] `cargo build -p buddy-whatsapp` succeeds
- [x] All existing tests pass

## Test Cases

- [x] Parse a config with `[interfaces.whatsapp]` and all fields populated; assert `WhatsAppConfig` has correct values
- [x] Parse a config without `[interfaces.whatsapp]`; assert defaults to `enabled: false`
- [x] Send `GET /webhook?hub.mode=subscribe&hub.verify_token=correct&hub.challenge=abc123`; assert response is `abc123`
- [x] Send `GET /webhook?hub.mode=subscribe&hub.verify_token=wrong&hub.challenge=abc123`; assert 403
- [x] Send `POST /webhook` with a valid text message payload; assert 200 and the message is parsed (check logs or a test hook)
- [x] Call `whatsapp_to_buddy()` with a text message payload; assert it returns a buddy-core `Message` with `Role::User` and the correct text
- [x] Call `whatsapp_to_buddy()` with a non-text payload (e.g., image); assert it returns `None`
- [x] Call `buddy_to_whatsapp()` with a text `Message`; assert it returns the text string
- [x] Call `WhatsAppClient::send_text_message()` against a mock HTTP server; assert the request has correct URL, headers, and body format
