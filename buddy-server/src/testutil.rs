//! Shared test utilities — mock providers, mock skills, and HTTP helpers.
//!
//! This module is gated with `#[cfg(test)]` in `main.rs` so it is excluded
//! from release builds.

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use chrono::Utc;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::api::{ChatEvent, ChatRequest};
use crate::provider::{Provider, ProviderError, Token, TokenStream};
use crate::skill::{Skill, SkillError};
use crate::types::{Message, MessageContent, Role};

// ── Mock skills ─────────────────────────────────────────────────────────

/// A skill that echoes its input. Consolidates the former `EchoSkill`
/// (api.rs) and `MockSkill` (skill/mod.rs).
pub struct MockEchoSkill;

impl Skill for MockEchoSkill {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Echoes input"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "value": { "type": "string" } },
            "required": ["value"]
        })
    }
    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let value = input
                .get("value")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidInput("missing required field: value".into()))?;
            Ok(serde_json::json!({ "echo": value }))
        })
    }
}

/// A skill that always fails with `SkillError::ExecutionFailed`.
pub struct FailingSkill;

impl Skill for FailingSkill {
    fn name(&self) -> &str {
        "failing"
    }
    fn description(&self) -> &str {
        "Always fails"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }
    fn execute(
        &self,
        _input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async { Err(SkillError::ExecutionFailed("boom".into())) })
    }
}

/// A no-op skill that returns `{ "ok": true }`. Used for multi-skill
/// registry tests.
pub struct MockNoOpSkill;

impl Skill for MockNoOpSkill {
    fn name(&self) -> &str {
        "noop"
    }
    fn description(&self) -> &str {
        "A no-op test skill"
    }
    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
        })
    }
    fn execute(
        &self,
        _input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async { Ok(serde_json::json!({ "ok": true })) })
    }
}

// ── Mock providers ──────────────────────────────────────────────────────

/// A simple mock provider that always returns the configured text tokens.
pub struct MockProvider {
    pub tokens: Vec<String>,
}

impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        let tokens = self.tokens.clone();
        let stream = async_stream::try_stream! {
            for text in tokens {
                yield Token::Text { text };
            }
        };
        Ok(Box::pin(stream))
    }
}

/// Responses the sequenced provider can return.
pub enum MockResponse {
    Text(Vec<String>),
    ToolCalls(Vec<(String, String, String)>),
}

/// A mock provider that returns different responses per call.
pub struct SequencedProvider {
    responses: Mutex<Vec<MockResponse>>,
}

impl SequencedProvider {
    pub fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

impl Provider for SequencedProvider {
    async fn complete(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        let response = {
            let mut q = self.responses.lock().unwrap();
            if q.is_empty() {
                MockResponse::Text(vec!["<no more responses>".into()])
            } else {
                q.remove(0)
            }
        };

        match response {
            MockResponse::Text(texts) => {
                let stream = async_stream::try_stream! {
                    for text in texts {
                        yield Token::Text { text };
                    }
                };
                Ok(Box::pin(stream))
            }
            MockResponse::ToolCalls(calls) => {
                let stream = async_stream::try_stream! {
                    for (id, name, arguments) in calls {
                        yield Token::ToolCall { id, name, arguments };
                    }
                };
                Ok(Box::pin(stream))
            }
        }
    }
}

// ── Configurable mock provider (for fallback chain tests) ───────────────

/// A mock provider whose behavior (succeed or fail) is fixed at construction.
pub enum ConfigurableMockProvider {
    Succeed(Vec<String>),
    FailNetwork(String),
}

impl Provider for ConfigurableMockProvider {
    async fn complete(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        match self {
            Self::Succeed(tokens) => {
                let tokens = tokens.clone();
                let stream = async_stream::try_stream! {
                    for text in tokens {
                        yield Token::Text { text };
                    }
                };
                Ok(Box::pin(stream))
            }
            Self::FailNetwork(msg) => Err(ProviderError::Network(msg.clone())),
        }
    }
}

// ── HTTP test helpers ───────────────────────────────────────────────────

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
    })
    .unwrap()
}

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

/// Post to /api/chat and return events excluding ConversationMeta.
pub async fn post_chat(app: Router, body: &str) -> Vec<ChatEvent> {
    post_chat_raw(app, body)
        .await
        .into_iter()
        .filter(|e| !matches!(e, ChatEvent::ConversationMeta { .. }))
        .collect()
}
