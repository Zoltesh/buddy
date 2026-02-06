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
use chrono::Utc;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::provider::{Provider, Token};
use crate::types::{Message, MessageContent, Role};

/// Maximum number of tool-call loop iterations before aborting.
const MAX_TOOL_ITERATIONS: usize = 10;

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
    ToolCallStart { id: String, name: String, arguments: String },
    ToolCallResult { id: String, content: String },
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
///
/// Implements the agentic tool-call loop: the LLM can request tool executions,
/// the backend runs them, feeds results back, and loops until a final text
/// response is produced or the iteration limit is reached.
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

    let tools = {
        let defs = state.registry.tool_definitions();
        if defs.is_empty() {
            None
        } else {
            Some(defs)
        }
    };

    // Channel for streaming events to the client.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatEvent>(64);

    tokio::spawn(async move {
        run_tool_loop(state, request.messages, tools, tx).await;
    });

    let events = async_stream::stream! {
        while let Some(event) = rx.recv().await {
            let is_done = matches!(event, ChatEvent::Done);
            yield Ok::<_, Infallible>(
                Event::default().data(serde_json::to_string(&event).unwrap())
            );
            if is_done {
                break;
            }
        }
    };

    Ok(Sse::new(events))
}

/// Run the tool-call loop, sending `ChatEvent`s through `tx`.
///
/// 1. Send messages + tool definitions to the provider.
/// 2. If the provider yields tool calls: execute them via the `SkillRegistry`,
///    append `ToolCall` and `ToolResult` messages, and call the provider again.
/// 3. Repeat until the provider returns only text (no tool calls).
/// 4. Text deltas are streamed to the client as `TokenDelta` events.
/// 5. Stops after `MAX_TOOL_ITERATIONS` to prevent runaway loops.
async fn run_tool_loop<P: Provider>(
    state: Arc<AppState<P>>,
    mut messages: Vec<Message>,
    tools: Option<Vec<serde_json::Value>>,
    tx: tokio::sync::mpsc::Sender<ChatEvent>,
) {
    for _iteration in 0..MAX_TOOL_ITERATIONS {
        // Call the provider.
        let token_stream = match state.provider.complete(messages.clone(), tools.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(ChatEvent::Error { message: e.to_string() }).await;
                let _ = tx.send(ChatEvent::Done).await;
                return;
            }
        };

        // Consume the stream, collecting text and tool calls.
        let mut tool_calls: Vec<(String, String, String)> = Vec::new();

        tokio::pin!(token_stream);
        while let Some(result) = token_stream.next().await {
            match result {
                Ok(Token::Text { text }) => {
                    // Stream text deltas immediately.
                    let _ = tx
                        .send(ChatEvent::TokenDelta {
                            content: text,
                        })
                        .await;
                }
                Ok(Token::ToolCall {
                    id,
                    name,
                    arguments,
                }) => {
                    tool_calls.push((id, name, arguments));
                }
                Err(e) => {
                    let _ = tx
                        .send(ChatEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                    let _ = tx.send(ChatEvent::Done).await;
                    return;
                }
            }
        }

        if tool_calls.is_empty() {
            // Final text response — done.
            let _ = tx.send(ChatEvent::Done).await;
            return;
        }

        // Execute each tool call.
        for (id, name, arguments) in &tool_calls {
            // Notify the client that a tool call is starting.
            let _ = tx
                .send(ChatEvent::ToolCallStart {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                })
                .await;

            // Append the assistant's tool call to the conversation.
            messages.push(Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                timestamp: Utc::now(),
            });

            // Execute the skill.
            let result_content = match state.registry.get(name) {
                Some(skill) => {
                    let input: serde_json::Value = serde_json::from_str(arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    match skill.execute(input).await {
                        Ok(output) => serde_json::to_string(&output)
                            .unwrap_or_else(|_| "{}".to_string()),
                        Err(e) => format!("Error: {e}"),
                    }
                }
                None => format!("Error: unknown tool '{name}'"),
            };

            // Notify the client of the result.
            let _ = tx
                .send(ChatEvent::ToolCallResult {
                    id: id.clone(),
                    content: result_content.clone(),
                })
                .await;

            // Append the tool result to the conversation.
            messages.push(Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: id.clone(),
                    content: result_content,
                },
                timestamp: Utc::now(),
            });
        }

        // Loop: call the provider again with updated messages.
    }

    // Exceeded the maximum number of iterations.
    let _ = tx
        .send(ChatEvent::Error {
            message: format!(
                "Tool call loop exceeded maximum of {MAX_TOOL_ITERATIONS} iterations"
            ),
        })
        .await;
    let _ = tx.send(ChatEvent::Done).await;
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
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;
    use tower::ServiceExt;
    use tower_http::services::ServeDir;

    use crate::provider::{ProviderError, Token, TokenStream};
    use crate::skill::{Skill, SkillError, SkillRegistry};
    use crate::types::{MessageContent, Role};

    // ── Simple mock provider (always returns text) ──────────────────────

    struct MockProvider {
        tokens: Vec<String>,
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

    // ── Sequenced mock provider (returns different responses per call) ──

    /// Responses the sequenced provider can return.
    enum MockResponse {
        Text(Vec<String>),
        ToolCalls(Vec<(String, String, String)>), // (id, name, arguments)
    }

    struct SequencedProvider {
        responses: Mutex<Vec<MockResponse>>,
    }

    impl SequencedProvider {
        fn new(responses: Vec<MockResponse>) -> Self {
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

    // ── Mock skill ──────────────────────────────────────────────────────

    struct EchoSkill;

    impl Skill for EchoSkill {
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
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>>
        {
            Box::pin(async move {
                let value = input["value"]
                    .as_str()
                    .ok_or_else(|| SkillError::InvalidInput("missing value".into()))?;
                Ok(serde_json::json!({ "echo": value }))
            })
        }
    }

    /// A skill that always fails.
    struct FailingSkill;

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
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>>
        {
            Box::pin(async { Err(SkillError::ExecutionFailed("boom".into())) })
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn registry_with_echo() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(EchoSkill));
        r
    }

    fn registry_with_failing() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(FailingSkill));
        r
    }

    fn test_app(tokens: Vec<String>) -> Router {
        let state = Arc::new(AppState {
            provider: MockProvider { tokens },
            registry: SkillRegistry::new(),
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
    }

    fn test_app_with_static(tokens: Vec<String>, static_dir: &str) -> Router {
        let state = Arc::new(AppState {
            provider: MockProvider { tokens },
            registry: SkillRegistry::new(),
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
            .fallback_service(ServeDir::new(static_dir))
    }

    fn sequenced_app(responses: Vec<MockResponse>, registry: SkillRegistry) -> Router {
        let state = Arc::new(AppState {
            provider: SequencedProvider::new(responses),
            registry,
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<SequencedProvider>))
            .with_state(state)
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

    async fn post_chat(app: Router, body: &str) -> Vec<ChatEvent> {
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

    // ── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn valid_request_streams_token_deltas_and_done() {
        let app = test_app(vec!["Hello".into(), " world".into()]);
        let events = post_chat(app, &make_chat_body()).await;

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

    // ── Tool-call loop tests ────────────────────────────────────────────

    #[tokio::test]
    async fn single_tool_call_executes_skill_and_returns_text() {
        let app = sequenced_app(
            vec![
                // First call: LLM requests a tool call.
                MockResponse::ToolCalls(vec![(
                    "call_1".into(),
                    "echo".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                // Second call: LLM returns text after seeing the tool result.
                MockResponse::Text(vec!["The echo said hello.".into()]),
            ],
            registry_with_echo(),
        );

        let events = post_chat(app, &make_chat_body()).await;

        // Expect: ToolCallStart, ToolCallResult, TokenDelta, Done
        assert!(events.contains(&ChatEvent::ToolCallStart {
            id: "call_1".into(),
            name: "echo".into(),
            arguments: r#"{"value":"hello"}"#.into(),
        }));
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { id, content }
            if id == "call_1" && content.contains("hello")
        )));
        assert!(events.contains(&ChatEvent::TokenDelta {
            content: "The echo said hello.".into(),
        }));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    #[tokio::test]
    async fn three_chained_tool_calls_all_execute() {
        let app = sequenced_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "echo".into(),
                    r#"{"value":"a"}"#.into(),
                )]),
                MockResponse::ToolCalls(vec![(
                    "c2".into(),
                    "echo".into(),
                    r#"{"value":"b"}"#.into(),
                )]),
                MockResponse::ToolCalls(vec![(
                    "c3".into(),
                    "echo".into(),
                    r#"{"value":"c"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done chaining.".into()]),
            ],
            registry_with_echo(),
        );

        let events = post_chat(app, &make_chat_body()).await;

        let starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ChatEvent::ToolCallStart { .. }))
            .collect();
        let results: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ChatEvent::ToolCallResult { .. }))
            .collect();

        assert_eq!(starts.len(), 3);
        assert_eq!(results.len(), 3);
        assert!(events.contains(&ChatEvent::TokenDelta {
            content: "Done chaining.".into(),
        }));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    #[tokio::test]
    async fn loop_stops_at_max_iterations() {
        // 11 consecutive tool calls — should stop at 10.
        let mut responses: Vec<MockResponse> = (0..11)
            .map(|i| {
                MockResponse::ToolCalls(vec![(
                    format!("c{i}"),
                    "echo".into(),
                    r#"{"value":"x"}"#.into(),
                )])
            })
            .collect();
        // Unreachable final text.
        responses.push(MockResponse::Text(vec!["never reached".into()]));

        let app = sequenced_app(responses, registry_with_echo());
        let events = post_chat(app, &make_chat_body()).await;

        let starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, ChatEvent::ToolCallStart { .. }))
            .collect();

        // Should execute exactly MAX_TOOL_ITERATIONS tool calls.
        assert_eq!(starts.len(), MAX_TOOL_ITERATIONS);

        // Should have an error about exceeding the limit.
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::Error { message } if message.contains("exceeded")
        )));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    #[tokio::test]
    async fn skill_error_is_fed_back_not_crash() {
        let app = sequenced_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "failing".into(),
                    "{}".into(),
                )]),
                MockResponse::Text(vec!["Handled the error.".into()]),
            ],
            registry_with_failing(),
        );

        let events = post_chat(app, &make_chat_body()).await;

        // The tool result should contain the error message.
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("Error:")
        )));

        // The conversation should continue — no fatal crash.
        assert!(events.contains(&ChatEvent::TokenDelta {
            content: "Handled the error.".into(),
        }));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    #[tokio::test]
    async fn unknown_tool_returns_error_result() {
        let app = sequenced_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "nonexistent".into(),
                    "{}".into(),
                )]),
                MockResponse::Text(vec!["OK.".into()]),
            ],
            SkillRegistry::new(), // empty registry
        );

        let events = post_chat(app, &make_chat_body()).await;

        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("unknown tool")
        )));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    #[tokio::test]
    async fn normal_chat_no_tools_works_unchanged() {
        // With an empty registry (no tools), behavior is v0.1-style.
        let app = test_app(vec!["Hello!".into()]);
        let events = post_chat(app, &make_chat_body()).await;

        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            ChatEvent::TokenDelta {
                content: "Hello!".into()
            }
        );
        assert_eq!(events[1], ChatEvent::Done);
    }

    #[tokio::test]
    async fn sse_stream_contains_tool_events() {
        let app = sequenced_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "tc1".into(),
                    "echo".into(),
                    r#"{"value":"test"}"#.into(),
                )]),
                MockResponse::Text(vec!["Final.".into()]),
            ],
            registry_with_echo(),
        );

        let events = post_chat(app, &make_chat_body()).await;

        let has_start = events.iter().any(|e| matches!(e, ChatEvent::ToolCallStart { .. }));
        let has_result = events.iter().any(|e| matches!(e, ChatEvent::ToolCallResult { .. }));

        assert!(has_start, "expected ToolCallStart in SSE stream");
        assert!(has_result, "expected ToolCallResult in SSE stream");
    }
}
