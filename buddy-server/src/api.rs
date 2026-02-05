//! HTTP API layer for buddy.
//!
//! ## Streaming transport: Server-Sent Events (SSE)
//!
//! V0.1 uses SSE via `POST /api/chat` for streaming responses. The client
//! sends a JSON `ChatRequest` and receives a stream of `ChatEvent` frames.
//!
//! SSE was chosen over WebSocket for V0.1 because:
//! - Standard HTTP semantics — malformed requests get proper 4xx status codes
//!   before any streaming begins
//! - Simpler client implementation (fetch + EventSource parsing)
//! - Works transparently with HTTP proxies and load balancers
//! - WebSocket can be added later if bidirectional communication is needed

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::provider::{Provider, ProviderError};
use crate::types::Message;

/// Incoming chat request.
#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
}

/// A single frame in the streamed response.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    TokenDelta { content: String },
    Done,
    Error { message: String },
}

/// Structured API error response.
#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Shared application state.
pub struct AppState<P> {
    pub provider: P,
    pub registry: crate::skill::SkillRegistry,
}

/// `POST /api/chat` — accepts a `ChatRequest` and streams `ChatEvent` frames via SSE.
pub async fn chat_handler<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    body: Bytes,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ApiError>)> {
    let request: ChatRequest = serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "bad_request".into(),
                message: format!("invalid request body: {e}"),
            }),
        )
    })?;

    let token_stream = state.provider.complete(request.messages).await.map_err(|e| {
        let (code, status) = match &e {
            ProviderError::Auth(_) => ("auth_error", StatusCode::UNAUTHORIZED),
            ProviderError::RateLimit(_) => ("rate_limit", StatusCode::TOO_MANY_REQUESTS),
            _ => ("provider_error", StatusCode::INTERNAL_SERVER_ERROR),
        };
        (
            status,
            Json(ApiError {
                code: code.into(),
                message: e.to_string(),
            }),
        )
    })?;

    let events = token_stream.map(|result| {
        let event = match result {
            Ok(token) => ChatEvent::TokenDelta { content: token.text },
            Err(e) => ChatEvent::Error { message: e.to_string() },
        };
        Ok(Event::default().data(serde_json::to_string(&event).unwrap()))
    });

    let done = futures_util::stream::once(async {
        Ok(Event::default().data(serde_json::to_string(&ChatEvent::Done).unwrap()))
    });

    Ok(Sse::new(events.chain(done)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::post;
    use axum::Router;
    use chrono::Utc;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use tower_http::services::ServeDir;

    use crate::provider::{Token, TokenStream};
    use crate::types::{MessageContent, Role};

    struct MockProvider {
        tokens: Vec<String>,
    }

    impl Provider for MockProvider {
        async fn complete(
            &self,
            _messages: Vec<Message>,
        ) -> Result<TokenStream, ProviderError> {
            let tokens = self.tokens.clone();
            let stream = async_stream::try_stream! {
                for text in tokens {
                    yield Token { text };
                }
            };
            Ok(Box::pin(stream))
        }
    }

    fn test_app(tokens: Vec<String>) -> Router {
        let state = Arc::new(AppState {
            provider: MockProvider { tokens },
            registry: crate::skill::SkillRegistry::new(),
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
    }

    fn test_app_with_static(tokens: Vec<String>, static_dir: &str) -> Router {
        let state = Arc::new(AppState {
            provider: MockProvider { tokens },
            registry: crate::skill::SkillRegistry::new(),
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
            .fallback_service(ServeDir::new(static_dir))
    }

    fn make_chat_body() -> String {
        serde_json::to_string(&ChatRequest {
            messages: vec![Message {
                role: Role::User,
                content: MessageContent::Text {
                    text: "Hi".into(),
                },
                timestamp: Utc::now(),
            }],
        })
        .unwrap()
    }

    fn parse_sse_events(body: &str) -> Vec<ChatEvent> {
        body.split("\n\n")
            .filter(|s| !s.is_empty())
            .filter_map(|chunk| {
                chunk
                    .strip_prefix("data: ")
                    .and_then(|data| serde_json::from_str(data).ok())
            })
            .collect()
    }

    #[tokio::test]
    async fn valid_request_streams_token_deltas_and_done() {
        let app = test_app(vec!["Hello".into(), " world".into()]);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(make_chat_body()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("text/event-stream"),
            "expected text/event-stream, got {ct}"
        );

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        let events = parse_sse_events(&body_str);

        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0],
            ChatEvent::TokenDelta {
                content: "Hello".into()
            }
        );
        assert_eq!(
            events[1],
            ChatEvent::TokenDelta {
                content: " world".into()
            }
        );
        assert_eq!(events[2], ChatEvent::Done);
    }

    #[tokio::test]
    async fn malformed_json_returns_400_with_structured_error() {
        let app = test_app(vec![]);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from("not valid json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(
            error.get("code").is_some(),
            "response should have 'code' field"
        );
        assert!(
            error.get("message").is_some(),
            "response should have 'message' field"
        );
        assert_eq!(error["code"], "bad_request");
    }

    #[tokio::test]
    async fn root_serves_index_html() {
        let dir = std::env::temp_dir().join("buddy-api-test-static");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<html><body>buddy</body></html>").unwrap();

        let app = test_app_with_static(vec![], dir.to_str().unwrap());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("text/html"), "expected text/html, got {ct}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn nonexistent_asset_returns_404() {
        let dir = std::env::temp_dir().join("buddy-api-test-404");
        std::fs::create_dir_all(&dir).unwrap();

        let app = test_app_with_static(vec![], dir.to_str().unwrap());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/assets/nonexistent.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        std::fs::remove_dir_all(&dir).ok();
    }
}
