//! HTTP test helpers for buddy-server tests.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use buddy_core::types::{Message, MessageContent, Role};
use chrono::Utc;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::api::{ChatEvent, ChatRequest};

/// Parse SSE response body into ChatEvents.
pub fn parse_sse_events(body: &str) -> Vec<ChatEvent> {
    body.split("\n\n")
        .filter(|s| !s.is_empty())
        .filter_map(|chunk| {
            chunk
                .strip_prefix("data: ")
                .and_then(|data| serde_json::from_str(data).ok())
        })
        .collect()
}

/// Create a minimal chat request body.
pub fn make_chat_body() -> String {
    serde_json::to_string(&ChatRequest {
        conversation_id: None,
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Text {
                text: "Hi".into(),
            },
            timestamp: Utc::now(),
        }],
        disable_memory: false,
    })
    .unwrap()
}

/// Create a chat request body with a conversation ID.
pub fn make_chat_body_with_conversation(conversation_id: &str) -> String {
    serde_json::to_string(&ChatRequest {
        conversation_id: Some(conversation_id.to_string()),
        messages: vec![Message {
            role: Role::User,
            content: MessageContent::Text {
                text: "Hi".into(),
            },
            timestamp: Utc::now(),
        }],
        disable_memory: false,
    })
    .unwrap()
}

/// Post to /api/chat and return all SSE events (including ConversationMeta).
pub async fn post_chat_raw(app: Router, body: &str) -> Vec<ChatEvent> {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/chat")
                .header("content-type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    parse_sse_events(&String::from_utf8(bytes.to_vec()).unwrap())
}

/// Post to /api/chat and return only non-meta events.
pub async fn post_chat(app: Router, body: &str) -> Vec<ChatEvent> {
    post_chat_raw(app, body)
        .await
        .into_iter()
        .filter(|e| !matches!(e, ChatEvent::ConversationMeta { .. }))
        .collect()
}
