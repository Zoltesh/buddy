# 005 — HTTP and WebSocket API

## Description

Implement the backend API layer that the frontend (and future clients) use to interact with buddy. This includes a REST endpoint for sending messages and a WebSocket endpoint for receiving streamed responses.

## Goal

A transport-agnostic API contract exists and is served over HTTP/WebSocket. The API is designed as if Telegram and WhatsApp clients already exist — nothing is web-UI-specific.

## Requirements

- **POST `/api/chat`** — accepts a JSON body with `messages: Vec<Message>`, returns a WebSocket upgrade or initiates streaming via SSE
  - Alternative: a dedicated **WebSocket endpoint at `/api/chat/ws`** that accepts messages as JSON frames and streams token frames back
- Pick one streaming transport (WebSocket or SSE) and commit to it for V0.1; document the choice and rationale
- Request/response types:
  - `ChatRequest { messages: Vec<Message> }`
  - Streamed response: a sequence of `ChatEvent` frames: `TokenDelta { content: String }`, `Done`, `Error { message: String }`
- Use `axum` as the HTTP framework
- The server serves the frontend static assets at `/` and the API at `/api/*`
- All API errors return structured JSON with a `code` and `message` field

## Acceptance Criteria

- [x] A client can connect and send a `ChatRequest`; receives a stream of `ChatEvent` frames ending with `Done`
- [x] Malformed requests return a structured JSON error, not a 500 or HTML
- [x] The `/` route serves the frontend `index.html`
- [x] Static assets under `/assets/*` are served correctly (JS, CSS)
- [x] The server starts and binds to the configured host/port from `Config`

## Test Cases

- Send a valid `ChatRequest` via the chosen streaming transport; assert at least one `TokenDelta` and a final `Done` event are received
- Send a malformed JSON body; assert a 400 response with a JSON error containing `"code"` and `"message"`
- `GET /` returns `200` with `Content-Type: text/html`
- `GET /assets/nonexistent.js` returns `404`
