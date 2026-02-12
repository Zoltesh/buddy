# 066 — WhatsApp Webhook Signature Verification

## Description

The `POST /webhook` endpoint in `buddy-whatsapp` currently accepts any request without verifying its origin. Meta signs every webhook payload with an HMAC-SHA256 signature using the app secret, sent in the `X-Hub-Signature-256` header. Without verifying this signature, an attacker who discovers the webhook URL can send forged messages.

This task adds signature verification to the webhook receiver so only payloads genuinely sent by Meta are processed.

## Goal

The WhatsApp webhook rejects requests with missing or invalid `X-Hub-Signature-256` headers, accepting only payloads whose signature matches the configured app secret.

## Requirements

- Add `app_secret_env` field to `WhatsAppConfig` (default: `"WHATSAPP_APP_SECRET"`)
  - The env var holds the Meta app secret used for HMAC-SHA256 verification
- On every `POST /webhook` request:
  1. Read the `X-Hub-Signature-256` header (format: `sha256=<hex digest>`)
  2. Compute HMAC-SHA256 of the raw request body using the app secret
  3. Compare the computed digest to the header value using constant-time comparison
  4. If missing or mismatched, return 403 Forbidden
  5. If valid, proceed with normal message processing
- Use the `hmac` and `sha2` crates for HMAC computation
- Use constant-time comparison (e.g., `subtle::ConstantTimeEq`) for the signature check
- If `app_secret_env` is not set or the env var is empty, log a warning at startup and skip signature verification (allow gradual adoption)

## Acceptance Criteria

- [x] `WhatsAppConfig` includes `app_secret_env` field with default
- [x] Valid signatures pass verification and messages are processed
- [x] Invalid signatures are rejected with 403
- [x] Missing `X-Hub-Signature-256` header is rejected with 403
- [x] When app secret is not configured, verification is skipped with a startup warning
- [x] All existing tests pass

## Test Cases

- [x] Send `POST /webhook` with a valid HMAC signature; assert 200
- [x] Send `POST /webhook` with an invalid signature; assert 403
- [x] Send `POST /webhook` with no `X-Hub-Signature-256` header; assert 403
- [x] Configure without app secret; assert webhook accepts unsigned requests (backward compatible)
