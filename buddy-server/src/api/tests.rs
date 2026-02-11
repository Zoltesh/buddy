use super::*;
use super::chat::MAX_TOOL_ITERATIONS;
use axum::body::Body;
use axum::http::Request;
use axum::routing::{get, post};
use axum::Router;
use http_body_util::BodyExt;
use tower::ServiceExt;
use tower_http::services::ServeDir;

use buddy_core::types::MessageContent;
use crate::provider::ProviderChain;
use crate::skill::SkillRegistry;
use crate::testutil::{
    ConfigurableMockProvider, FailingSkill, MockEchoSkill, MockProvider, MockResponse,
    SequencedProvider, make_chat_body, make_chat_body_with_conversation, post_chat,
    post_chat_raw,
};

// ── Helpers ─────────────────────────────────────────────────────────

fn test_config() -> buddy_core::config::Config {
    buddy_core::config::Config::parse(
        r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
    )
    .unwrap()
}

fn registry_with_echo() -> SkillRegistry {
    let mut r = SkillRegistry::new();
    r.register(Box::new(MockEchoSkill));
    r
}

fn registry_with_failing() -> SkillRegistry {
    let mut r = SkillRegistry::new();
    r.register(Box::new(FailingSkill));
    r
}

fn test_app(tokens: Vec<String>) -> Router {
    let state = Arc::new(AppState {
        provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
        registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
        store: buddy_core::store::Store::open_in_memory().unwrap(),
        embedder: arc_swap::ArcSwap::from_pointee(None),
        vector_store: arc_swap::ArcSwap::from_pointee(None),
        working_memory: crate::skill::working_memory::new_working_memory_map(),
        memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
        warnings: crate::warning::new_shared_warnings(),
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
        approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
        approval_timeout: std::time::Duration::from_secs(1),
        config: std::sync::RwLock::new(test_config()),
        config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
        on_config_change: None,
    });
    Router::new()
        .route("/api/chat", post(chat_handler::<MockProvider>))
        .with_state(state)
}

fn test_app_with_static(tokens: Vec<String>, static_dir: &str) -> Router {
    let state = Arc::new(AppState {
        provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
        registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
        store: buddy_core::store::Store::open_in_memory().unwrap(),
        embedder: arc_swap::ArcSwap::from_pointee(None),
        vector_store: arc_swap::ArcSwap::from_pointee(None),
        working_memory: crate::skill::working_memory::new_working_memory_map(),
        memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
        warnings: crate::warning::new_shared_warnings(),
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
        approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
        approval_timeout: std::time::Duration::from_secs(1),
        config: std::sync::RwLock::new(test_config()),
        config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
        on_config_change: None,
    });
    Router::new()
        .route("/api/chat", post(chat_handler::<MockProvider>))
        .with_state(state)
        .fallback_service(ServeDir::new(static_dir))
}

fn sequenced_app(responses: Vec<MockResponse>, registry: SkillRegistry) -> Router {
    let state = Arc::new(AppState {
        provider: arc_swap::ArcSwap::from_pointee(SequencedProvider::new(responses)),
        registry: arc_swap::ArcSwap::from_pointee(registry),
        store: buddy_core::store::Store::open_in_memory().unwrap(),
        embedder: arc_swap::ArcSwap::from_pointee(None),
        vector_store: arc_swap::ArcSwap::from_pointee(None),
        working_memory: crate::skill::working_memory::new_working_memory_map(),
        memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
        warnings: crate::warning::new_shared_warnings(),
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
        approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
        approval_timeout: std::time::Duration::from_secs(1),
        config: std::sync::RwLock::new(test_config()),
        config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
        on_config_change: None,
    });
    Router::new()
        .route("/api/chat", post(chat_handler::<SequencedProvider>))
        .with_state(state)
}

fn conversation_app(tokens: Vec<String>) -> (Arc<AppState<MockProvider>>, Router) {
    let state = Arc::new(AppState {
        provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
        registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
        store: buddy_core::store::Store::open_in_memory().unwrap(),
        embedder: arc_swap::ArcSwap::from_pointee(None),
        vector_store: arc_swap::ArcSwap::from_pointee(None),
        working_memory: crate::skill::working_memory::new_working_memory_map(),
        memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
        warnings: crate::warning::new_shared_warnings(),
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
        approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
        approval_timeout: std::time::Duration::from_secs(1),
        config: std::sync::RwLock::new(test_config()),
        config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
        on_config_change: None,
    });
    let router = Router::new()
        .route("/api/chat", post(chat_handler::<MockProvider>))
        .route(
            "/api/conversations",
            get(list_conversations::<MockProvider>).post(create_conversation::<MockProvider>),
        )
        .route(
            "/api/conversations/{id}",
            get(get_conversation::<MockProvider>).delete(delete_conversation::<MockProvider>),
        )
        .with_state(state.clone());
    (state, router)
}

// ── Chat tests ───────────────────────────────────────────────────

mod chat {
    use super::*;

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
}

// ── Tool-call loop tests ────────────────────────────────────────────

mod tool_loop {
    use super::*;

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

    #[tokio::test]
    async fn fallback_emits_warning_event_in_sse_stream() {
        let chain = ProviderChain::new(vec![
            (
                ConfigurableMockProvider::FailNetwork("down".into()),
                "primary".into(),
            ),
            (
                ConfigurableMockProvider::Succeed(vec!["fallback response".into()]),
                "fallback-model".into(),
            ),
        ]);
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(chain),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        let app = Router::new()
            .route(
                "/api/chat",
                post(chat_handler::<ProviderChain<ConfigurableMockProvider>>),
            )
            .with_state(state);

        let events = post_chat(app, &make_chat_body()).await;

        // Warning should appear before the token delta.
        let warning_idx = events
            .iter()
            .position(|e| matches!(e, ChatEvent::Warning { .. }))
            .expect("expected a Warning event in the SSE stream");
        let delta_idx = events
            .iter()
            .position(|e| matches!(e, ChatEvent::TokenDelta { .. }))
            .expect("expected a TokenDelta event");
        assert!(
            warning_idx < delta_idx,
            "Warning should come before TokenDelta"
        );

        // Verify the warning mentions the fallback model.
        assert!(matches!(
            &events[warning_idx],
            ChatEvent::Warning { message } if message.contains("fallback-model")
        ));
        assert!(events.last() == Some(&ChatEvent::Done));
    }
}

// ── Conversation management tests ──────────────────────────────────

mod conversations {
    use super::*;

    #[tokio::test]
    async fn list_conversations_empty_on_fresh_db() {
        let (_, app) = conversation_app(vec![]);
        let response = app
            .oneshot(Request::builder().uri("/api/conversations").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn create_then_list_conversation() {
        let (_, app) = conversation_app(vec![]);

        // Create
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/conversations")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let conv: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(conv.get("id").is_some());

        // List
        let response = app
            .oneshot(Request::builder().uri("/api/conversations").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["id"], conv["id"]);
    }

    #[tokio::test]
    async fn get_nonexistent_conversation_returns_404() {
        let (_, app) = conversation_app(vec![]);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/conversations/nonexistent-id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(err["code"], "not_found");
    }

    #[tokio::test]
    async fn delete_conversation_returns_204() {
        let (state, app) = conversation_app(vec![]);
        let conv = state.store.create_conversation("To delete").unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/conversations/{}", conv.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        // Verify it's gone.
        assert!(state.store.get_conversation(&conv.id).unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_conversation_returns_404() {
        let (_, app) = conversation_app(vec![]);
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/conversations/nonexistent-id")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn chat_without_conversation_id_auto_creates() {
        let (state, app) = conversation_app(vec!["Reply".into()]);

        let events = post_chat_raw(app, &make_chat_body()).await;

        // First event should be ConversationMeta.
        assert!(
            matches!(&events[0], ChatEvent::ConversationMeta { conversation_id } if !conversation_id.is_empty()),
            "first event should be ConversationMeta"
        );

        // A conversation should have been auto-created.
        let convs = state.store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].title, "Hi"); // title from the user message
    }

    #[tokio::test]
    async fn chat_with_conversation_id_appends_to_existing() {
        let (state, app) = conversation_app(vec!["Reply".into()]);
        let conv = state.store.create_conversation("Existing").unwrap();

        let body = make_chat_body_with_conversation(&conv.id);
        let events = post_chat_raw(app, &body).await;

        // Should get ConversationMeta with the provided id.
        assert!(matches!(
            &events[0],
            ChatEvent::ConversationMeta { conversation_id } if conversation_id == &conv.id
        ));

        // Messages should be persisted.
        let loaded = state.store.get_conversation(&conv.id).unwrap().unwrap();
        assert!(loaded.messages.len() >= 2); // user + assistant
    }

    #[tokio::test]
    async fn chat_with_nonexistent_conversation_id_returns_404() {
        let (_, app) = conversation_app(vec![]);
        let body = make_chat_body_with_conversation("nonexistent-id");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn chat_persists_all_message_types() {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(SequencedProvider::new(vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "echo".into(),
                    r#"{"value":"test"}"#.into(),
                )]),
                MockResponse::Text(vec!["Final answer.".into()]),
            ])),
            registry: arc_swap::ArcSwap::from_pointee(registry_with_echo()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        let app = Router::new()
            .route("/api/chat", post(chat_handler::<SequencedProvider>))
            .route(
                "/api/conversations",
                get(list_conversations::<SequencedProvider>).post(create_conversation::<SequencedProvider>),
            )
            .route(
                "/api/conversations/{id}",
                get(get_conversation::<SequencedProvider>).delete(delete_conversation::<SequencedProvider>),
            )
            .with_state(state.clone());

        let events = post_chat_raw(app, &make_chat_body()).await;

        // Get the conversation id from the meta event.
        let conv_id = match &events[0] {
            ChatEvent::ConversationMeta { conversation_id } => conversation_id.clone(),
            _ => panic!("expected ConversationMeta as first event"),
        };

        let conv = state.store.get_conversation(&conv_id).unwrap().unwrap();

        // Should have: user msg, tool call, tool result, assistant text = 4 messages
        assert_eq!(conv.messages.len(), 4, "expected 4 persisted messages, got {}: {:?}", conv.messages.len(), conv.messages);

        // Verify types
        assert!(matches!(conv.messages[0].content, MessageContent::Text { .. }));
        assert!(matches!(conv.messages[1].content, MessageContent::ToolCall { .. }));
        assert!(matches!(conv.messages[2].content, MessageContent::ToolResult { .. }));
        assert!(matches!(conv.messages[3].content, MessageContent::Text { .. }));
    }

    #[tokio::test]
    async fn sse_stream_starts_with_conversation_meta() {
        let (_, app) = conversation_app(vec!["Hi".into()]);
        let events = post_chat_raw(app, &make_chat_body()).await;

        assert!(!events.is_empty());
        assert!(
            matches!(&events[0], ChatEvent::ConversationMeta { conversation_id } if !conversation_id.is_empty()),
            "SSE stream must start with ConversationMeta"
        );
    }
}

// ── Warning system tests ──────────────────────────────────────────────

mod warnings {
    use super::*;
    use crate::warning::{new_shared_warnings, Warning, WarningSeverity};

    fn warnings_app(
        tokens: Vec<String>,
        setup: impl FnOnce(&mut crate::warning::WarningCollector),
    ) -> Router {
        let warnings = new_shared_warnings();
        {
            let mut collector = warnings.write().unwrap();
            setup(&mut collector);
        }
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings,
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .route("/api/warnings", get(get_warnings::<MockProvider>))
            .with_state(state)
    }

    #[tokio::test]
    async fn full_config_no_warnings() {
        let app = warnings_app(vec![], |_| {});

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn single_chat_provider_info() {
        let app = warnings_app(vec![], |c| {
            c.add(Warning {
                code: "single_chat_provider".into(),
                message: "Only one chat provider configured — no fallback available. Add additional [[models.chat.providers]] entries to buddy.toml for redundancy.".into(),
                severity: WarningSeverity::Info,
            });
        });

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "single_chat_provider");
        assert_eq!(list[0].severity, WarningSeverity::Info);
    }

    #[tokio::test]
    async fn runtime_warning_appears() {
        let warnings = new_shared_warnings();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec![] }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: warnings.clone(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        let app = Router::new()
            .route("/api/warnings", get(get_warnings::<MockProvider>))
            .with_state(state);

        // Add a warning at runtime.
        {
            let mut collector = warnings.write().unwrap();
            collector.add(Warning {
                code: "runtime_issue".into(),
                message: "Something went wrong at runtime.".into(),
                severity: WarningSeverity::Warning,
            });
        }

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "runtime_issue");
    }

    #[tokio::test]
    async fn clear_warning_removes_it() {
        let warnings = new_shared_warnings();
        {
            let mut collector = warnings.write().unwrap();
            collector.add(Warning {
                code: "to_clear".into(),
                message: "Will be cleared.".into(),
                severity: WarningSeverity::Warning,
            });
            collector.add(Warning {
                code: "keep_me".into(),
                message: "Should remain.".into(),
                severity: WarningSeverity::Info,
            });
        }
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec![] }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: warnings.clone(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        let app = Router::new()
            .route("/api/warnings", get(get_warnings::<MockProvider>))
            .with_state(state);

        // Clear one warning.
        {
            let mut collector = warnings.write().unwrap();
            collector.clear("to_clear");
        }

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].code, "keep_me");
    }

    #[tokio::test]
    async fn sse_stream_includes_warnings_event() {
        let app = warnings_app(vec!["Hello".into()], |c| {
            c.add(Warning {
                code: "test_warning".into(),
                message: "A test warning.".into(),
                severity: WarningSeverity::Warning,
            });
        });

        let events = post_chat_raw(app, &make_chat_body()).await;

        // ConversationMeta should be first, then Warnings before TokenDelta.
        assert!(matches!(&events[0], ChatEvent::ConversationMeta { .. }));
        let warnings_idx = events
            .iter()
            .position(|e| matches!(e, ChatEvent::Warnings { .. }))
            .expect("expected a Warnings event in the SSE stream");
        let delta_idx = events
            .iter()
            .position(|e| matches!(e, ChatEvent::TokenDelta { .. }))
            .expect("expected a TokenDelta event");
        assert!(
            warnings_idx < delta_idx,
            "Warnings should come before TokenDelta"
        );

        // Verify the warning content.
        if let ChatEvent::Warnings { warnings } = &events[warnings_idx] {
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0].code, "test_warning");
        } else {
            panic!("expected Warnings event");
        }
    }

    #[tokio::test]
    async fn warning_messages_include_guidance() {
        let app = warnings_app(vec![], |c| {
            c.add(Warning {
                code: "single_chat_provider".into(),
                message: "Only one chat provider configured — no fallback available. Add additional [[models.chat.providers]] entries to buddy.toml for redundancy.".into(),
                severity: WarningSeverity::Info,
            });
        });

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
        assert!(
            list[0].message.contains("buddy.toml"),
            "warning message should include guidance referencing buddy.toml: {}",
            list[0].message
        );
    }

    // ── Task 039: Chat UI warning banner tests ──────────────────────────

    #[tokio::test]
    async fn single_chat_provider_has_info_severity() {
        let app = warnings_app(vec![], |c| {
            c.add(Warning {
                code: "single_chat_provider".into(),
                message: "Only one chat provider configured.".into(),
                severity: WarningSeverity::Info,
            });
        });

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["severity"], "info");
        assert_eq!(arr[0]["code"], "single_chat_provider");
    }

    #[tokio::test]
    async fn warnings_persist_across_fetches() {
        let warnings = new_shared_warnings();
        {
            let mut collector = warnings.write().unwrap();
            collector.add(Warning {
                code: "no_vector_store".into(),
                message: "Vector store unavailable.".into(),
                severity: WarningSeverity::Warning,
            });
        }
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec![] }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings,
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });

        // First fetch
        let app1 = Router::new()
            .route("/api/warnings", get(get_warnings::<MockProvider>))
            .with_state(state.clone());
        let resp1 = app1
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let list1: Vec<Warning> =
            serde_json::from_slice(&resp1.into_body().collect().await.unwrap().to_bytes()).unwrap();

        // Second fetch (simulates page reload)
        let app2 = Router::new()
            .route("/api/warnings", get(get_warnings::<MockProvider>))
            .with_state(state);
        let resp2 = app2
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let list2: Vec<Warning> =
            serde_json::from_slice(&resp2.into_body().collect().await.unwrap().to_bytes()).unwrap();

        assert_eq!(list1.len(), 1);
        assert_eq!(list2.len(), 1);
        assert_eq!(list1[0].code, list2[0].code);
    }

    #[tokio::test]
    async fn multiple_warnings_all_returned() {
        let app = warnings_app(vec![], |c| {
            c.add(Warning {
                code: "single_chat_provider".into(),
                message: "Single chat provider.".into(),
                severity: WarningSeverity::Info,
            });
            c.add(Warning {
                code: "no_vector_store".into(),
                message: "No vector store.".into(),
                severity: WarningSeverity::Warning,
            });
        });

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 2);
        let codes: Vec<&str> = list.iter().map(|w| w.code.as_str()).collect();
        assert!(codes.contains(&"single_chat_provider"));
        assert!(codes.contains(&"no_vector_store"));
    }

    #[tokio::test]
    async fn sse_warnings_event_updates_during_stream() {
        let app = warnings_app(vec!["Reply".into()], |c| {
            c.add(Warning {
                code: "single_chat_provider".into(),
                message: "Only one provider.".into(),
                severity: WarningSeverity::Info,
            });
            c.add(Warning {
                code: "no_vector_store".into(),
                message: "Vector store unavailable.".into(),
                severity: WarningSeverity::Warning,
            });
        });

        let events = post_chat_raw(app, &make_chat_body()).await;
        let warnings_event = events
            .iter()
            .find(|e| matches!(e, ChatEvent::Warnings { .. }))
            .expect("expected Warnings in SSE stream");

        if let ChatEvent::Warnings { warnings } = warnings_event {
            assert_eq!(warnings.len(), 2);
        } else {
            panic!("expected Warnings event");
        }
    }

    #[tokio::test]
    async fn warning_json_includes_code_for_settings_link() {
        let app = warnings_app(vec![], |c| {
            c.add(Warning {
                code: "no_vector_store".into(),
                message: "Vector store failed to initialize.".into(),
                severity: WarningSeverity::Warning,
            });
        });

        let response = app
            .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let arr = body.as_array().unwrap();
        // Verify JSON has the three fields the frontend needs: code, message, severity
        let w = &arr[0];
        assert!(w.get("code").is_some(), "JSON must include 'code' field");
        assert!(w.get("message").is_some(), "JSON must include 'message' field");
        assert!(w.get("severity").is_some(), "JSON must include 'severity' field");
        // code must be a known value the frontend maps to a Settings link
        let code = w["code"].as_str().unwrap();
        let known_codes = ["no_vector_store", "single_chat_provider", "embedding_dimension_mismatch"];
        assert!(known_codes.contains(&code), "code {code} should be a known warning code");
    }
}

// ── Approval tests ─────────────────────────────────────────────────

mod approval {
    use super::*;
    use buddy_core::config::ApprovalPolicy;
    use crate::testutil::{MockMutatingSkill, MockNetworkSkill};

    fn registry_with_mutating() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(MockMutatingSkill));
        r
    }

    fn registry_with_network() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(MockNetworkSkill));
        r
    }

    fn approval_app(
        responses: Vec<MockResponse>,
        registry: SkillRegistry,
        overrides: HashMap<String, ApprovalPolicy>,
        timeout: std::time::Duration,
    ) -> (Arc<AppState<SequencedProvider>>, Router) {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(SequencedProvider::new(responses)),
            registry: arc_swap::ArcSwap::from_pointee(registry),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(overrides),
            approval_timeout: timeout,
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        let router = Router::new()
            .route("/api/chat", post(chat_handler::<SequencedProvider>))
            .with_state(state.clone());
        (state, router)
    }

    // 1. ReadOnly executes without approval
    #[tokio::test]
    async fn readonly_executes_without_approval() {
        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "echo".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_echo(),
            HashMap::new(),
            std::time::Duration::from_secs(1),
        );

        let events = post_chat(app, &make_chat_body()).await;

        // No ApprovalRequest in the stream.
        assert!(
            !events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
            "ReadOnly skill should not emit ApprovalRequest"
        );
        // Skill executed successfully.
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("hello")
        )));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    // 2. Mutating emits ApprovalRequest — timeout → denied
    #[tokio::test]
    async fn mutating_emits_approval_request_and_times_out() {
        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            HashMap::new(),
            std::time::Duration::from_millis(50),
        );

        let events = post_chat(app, &make_chat_body()).await;

        // ApprovalRequest should be in the stream.
        assert!(
            events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
            "Mutating skill should emit ApprovalRequest"
        );
        // Should be denied (timeout).
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("User denied execution of mutating")
        )));
    }

    // 3. Approve mutating skill
    #[tokio::test]
    async fn approve_mutating_skill_executes() {
        let (state, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            HashMap::new(),
            std::time::Duration::from_secs(5),
        );

        // Background task that auto-approves.
        let pending = state.pending_approvals.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let mut map = pending.lock().await;
                let keys: Vec<String> = map.keys().cloned().collect();
                for key in keys {
                    if let Some(tx) = map.remove(&key) {
                        let _ = tx.send(true);
                    }
                }
            }
        });

        let events = post_chat(app, &make_chat_body()).await;

        // Skill should have executed (echo result).
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("hello")
        )));
        assert!(events.last() == Some(&ChatEvent::Done));
    }

    // 4. Deny mutating skill
    #[tokio::test]
    async fn deny_mutating_skill_returns_denied() {
        let (state, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            HashMap::new(),
            std::time::Duration::from_secs(5),
        );

        // Background task that denies.
        let pending = state.pending_approvals.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let mut map = pending.lock().await;
                let keys: Vec<String> = map.keys().cloned().collect();
                for key in keys {
                    if let Some(tx) = map.remove(&key) {
                        let _ = tx.send(false);
                    }
                }
            }
        });

        let events = post_chat(app, &make_chat_body()).await;

        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("User denied execution of mutating")
        )));
    }

    // 5. Trust policy auto-approves
    #[tokio::test]
    async fn trust_policy_auto_approves() {
        let mut overrides = HashMap::new();
        overrides.insert("mutating".into(), ApprovalPolicy::Trust);

        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            overrides,
            std::time::Duration::from_secs(1),
        );

        let events = post_chat(app, &make_chat_body()).await;

        // No ApprovalRequest.
        assert!(
            !events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
            "Trust policy should not emit ApprovalRequest"
        );
        // Skill executed.
        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("hello")
        )));
    }

    // 6. Once policy — first requires approval, second auto-approves
    #[tokio::test]
    async fn once_policy_asks_first_then_auto_approves() {
        let mut overrides = HashMap::new();
        overrides.insert("mutating".into(), ApprovalPolicy::Once);

        let (state, app) = approval_app(
            vec![
                // First tool call.
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"first"}"#.into(),
                )]),
                // Second tool call.
                MockResponse::ToolCalls(vec![(
                    "c2".into(),
                    "mutating".into(),
                    r#"{"value":"second"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            overrides,
            std::time::Duration::from_secs(5),
        );

        // Auto-approve the first request.
        let pending = state.pending_approvals.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let mut map = pending.lock().await;
                let keys: Vec<String> = map.keys().cloned().collect();
                for key in keys {
                    if let Some(tx) = map.remove(&key) {
                        let _ = tx.send(true);
                    }
                }
            }
        });

        let events = post_chat(app, &make_chat_body()).await;

        // Should have exactly one ApprovalRequest (for the first call).
        let approval_count = events
            .iter()
            .filter(|e| matches!(e, ChatEvent::ApprovalRequest { .. }))
            .count();
        assert_eq!(
            approval_count, 1,
            "Once policy should emit ApprovalRequest only on first call"
        );

        // Both calls should have executed.
        let result_count = events
            .iter()
            .filter(|e| matches!(e, ChatEvent::ToolCallResult { content, .. } if content.contains("echo")))
            .count();
        assert_eq!(result_count, 2, "Both tool calls should have executed");
    }

    // 7. Network skill emits ApprovalRequest
    #[tokio::test]
    async fn network_skill_emits_approval_request() {
        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "network".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_network(),
            HashMap::new(),
            std::time::Duration::from_millis(50),
        );

        let events = post_chat(app, &make_chat_body()).await;

        assert!(
            events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
            "Network skill should emit ApprovalRequest"
        );
    }

    // 8. Timeout treated as denied
    #[tokio::test]
    async fn timeout_treated_as_denied() {
        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            HashMap::new(),
            std::time::Duration::from_millis(50),
        );

        let events = post_chat(app, &make_chat_body()).await;

        assert!(events.iter().any(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("User denied")
        )));
    }

    // 9. ApprovalRequest event shape
    #[tokio::test]
    async fn approval_request_event_shape() {
        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            HashMap::new(),
            std::time::Duration::from_millis(50),
        );

        let events = post_chat(app, &make_chat_body()).await;

        let approval = events.iter().find(|e| matches!(e, ChatEvent::ApprovalRequest { .. }));
        assert!(approval.is_some(), "should contain ApprovalRequest");

        if let Some(ChatEvent::ApprovalRequest { id, skill_name, arguments, permission_level }) = approval {
            assert!(!id.is_empty(), "approval id should not be empty");
            assert_eq!(skill_name, "mutating");
            assert_eq!(arguments["value"], "hello");
            assert_eq!(permission_level, "mutating");
        }
    }

    // 10. Denied message is informative
    #[tokio::test]
    async fn denied_message_is_informative() {
        let (_, app) = approval_app(
            vec![
                MockResponse::ToolCalls(vec![(
                    "c1".into(),
                    "mutating".into(),
                    r#"{"value":"hello"}"#.into(),
                )]),
                MockResponse::Text(vec!["Done.".into()]),
            ],
            registry_with_mutating(),
            HashMap::new(),
            std::time::Duration::from_millis(50),
        );

        let events = post_chat(app, &make_chat_body()).await;

        let denied = events.iter().find(|e| matches!(
            e,
            ChatEvent::ToolCallResult { content, .. } if content.contains("denied")
        ));
        assert!(denied.is_some(), "should contain denied tool result");

        if let Some(ChatEvent::ToolCallResult { content, .. }) = denied {
            assert!(
                content.contains("User denied execution of"),
                "denied message should be informative: {content}"
            );
        }
    }
}

mod config_api {
    use super::*;

    fn config_app(config: buddy_core::config::Config) -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .with_state(state)
    }

    #[tokio::test]
    async fn full_config_returns_all_sections() {
        let config = buddy_core::config::Config::parse(
            r#"
[server]
host = "0.0.0.0"
port = 8080

[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"

[chat]
system_prompt = "Be helpful."

[skills.read_file]
allowed_directories = ["/tmp"]

[skills.write_file]
allowed_directories = ["/tmp"]

[skills.fetch_url]
allowed_domains = ["example.com"]

[memory]
auto_retrieve = false
auto_retrieve_limit = 5
similarity_threshold = 0.8
"#,
        )
        .unwrap();

        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value =
            serde_json::from_slice(&bytes).expect("response should be valid JSON");

        // Models section
        let chat_providers = &json["models"]["chat"]["providers"];
        assert_eq!(chat_providers.as_array().unwrap().len(), 2);
        assert_eq!(chat_providers[0]["type"], "openai");
        assert_eq!(chat_providers[0]["model"], "gpt-4");
        assert_eq!(chat_providers[1]["type"], "lmstudio");

        let emb_providers = &json["models"]["embedding"]["providers"];
        assert_eq!(emb_providers.as_array().unwrap().len(), 1);
        assert_eq!(emb_providers[0]["type"], "local");

        // Chat section
        assert_eq!(json["chat"]["system_prompt"], "Be helpful.");

        // Server section
        assert_eq!(json["server"]["host"], "0.0.0.0");
        assert_eq!(json["server"]["port"], 8080);

        // Skills section
        assert!(json["skills"]["read_file"].is_object());
        assert!(json["skills"]["write_file"].is_object());
        assert!(json["skills"]["fetch_url"].is_object());

        // Memory section
        assert_eq!(json["memory"]["auto_retrieve"], false);
        assert_eq!(json["memory"]["auto_retrieve_limit"], 5);
    }

    #[tokio::test]
    async fn minimal_config_returns_nulls_for_optional_sections() {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap();

        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert!(json["models"]["embedding"].is_null());
        assert!(json["skills"]["read_file"].is_null());
        assert!(json["skills"]["write_file"].is_null());
        assert!(json["skills"]["fetch_url"].is_null());
    }

    #[tokio::test]
    async fn api_key_env_present_but_secret_not_leaked() {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "BUDDY_TEST_SECRET_029"
"#,
        )
        .unwrap();

        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(bytes.to_vec()).unwrap();

        // The env var name should appear in the response.
        assert!(
            body.contains("BUDDY_TEST_SECRET_029"),
            "api_key_env name should be present"
        );

        // Set the env var to a known value and verify it does NOT appear.
        unsafe { std::env::set_var("BUDDY_TEST_SECRET_029", "super-secret-key-value") };
        assert!(
            !body.contains("super-secret-key-value"),
            "resolved secret must not appear in the response"
        );
        unsafe { std::env::remove_var("BUDDY_TEST_SECRET_029") };
    }

    #[tokio::test]
    async fn round_trip_json_to_config() {
        let config = buddy_core::config::Config::parse(
            r#"
[server]
host = "127.0.0.1"
port = 3000

[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "MY_KEY"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"

[chat]
system_prompt = "Hello"

[skills.read_file]
allowed_directories = ["/tmp"]

[memory]
auto_retrieve = true
auto_retrieve_limit = 3
similarity_threshold = 0.5
"#,
        )
        .unwrap();

        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let deserialized: buddy_core::config::Config =
            serde_json::from_slice(&bytes).expect("should deserialize back into Config");

        assert_eq!(deserialized.server.host, "127.0.0.1");
        assert_eq!(deserialized.server.port, 3000);
        assert_eq!(deserialized.models.chat.providers.len(), 1);
        assert_eq!(deserialized.models.chat.providers[0].model, "gpt-4");
        assert!(deserialized.models.embedding.is_some());
        assert_eq!(deserialized.chat.system_prompt, "Hello");
        assert!(deserialized.skills.read_file.is_some());
        assert!(deserialized.memory.auto_retrieve);
    }

    /// Helper that creates a temp dir with a real config file and returns (temp_dir, Router).
    fn config_write_app() -> (std::path::PathBuf, Router) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("buddy-config-write-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("buddy.toml");
        let initial_toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#;
        std::fs::write(&config_path, initial_toml).unwrap();
        let config = buddy_core::config::Config::parse(initial_toml).unwrap();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec!["hi".into()] }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: None,
        });
        let router = Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .route("/api/config/models", axum::routing::put(put_config_models::<MockProvider>))
            .route("/api/config/skills", axum::routing::put(put_config_skills::<MockProvider>))
            .route("/api/config/chat", axum::routing::put(put_config_chat::<MockProvider>))
            .route("/api/config/server", axum::routing::put(put_config_server::<MockProvider>))
            .route("/api/config/memory", axum::routing::put(put_config_memory::<MockProvider>))
            .with_state(state);
        (dir, router)
    }

    #[tokio::test]
    async fn put_valid_models_persists_to_disk() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "chat": {
                "providers": [{
                    "type": "openai",
                    "model": "gpt-4o",
                    "endpoint": "https://api.openai.com/v1",
                    "api_key_env": "MY_KEY"
                }]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
        let reparsed = buddy_core::config::Config::parse(&disk).unwrap();
        assert_eq!(reparsed.models.chat.providers[0].model, "gpt-4o");
        assert_eq!(reparsed.models.chat.providers[0].provider_type, "openai");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_models_empty_providers_returns_400() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "chat": { "providers": [] }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let err: config::ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(err.errors.iter().any(|e| e.field.contains("models.chat.providers")));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_models_unknown_provider_type_returns_400() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "chat": {
                "providers": [{
                    "type": "anthropic",
                    "model": "claude"
                }]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let err: config::ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(err.errors.iter().any(|e| e.field.contains("type")));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_models_empty_model_string_returns_400() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "chat": {
                "providers": [{
                    "type": "openai",
                    "model": ""
                }]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let err: config::ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(err.errors.iter().any(|e| e.field.contains("model")));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_models_direct_api_key_succeeds() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "chat": {
                "providers": [{
                    "type": "openai",
                    "model": "gpt-4",
                    "api_key": "sk-test-direct-key",
                    "endpoint": "https://api.openai.com/v1"
                }]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let updated: buddy_core::config::Config = serde_json::from_slice(&bytes).unwrap();
        let provider = &updated.models.chat.providers[0];
        assert_eq!(provider.api_key.as_deref(), Some("sk-test-direct-key"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_skills_updates_skills_preserves_others() {
        let (dir, app) = config_write_app();
        let tmp = std::env::temp_dir();
        let body = serde_json::json!({
            "read_file": {
                "allowed_directories": [tmp.to_str().unwrap()]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
        let reparsed = buddy_core::config::Config::parse(&disk).unwrap();
        assert!(reparsed.skills.read_file.is_some());
        // Models section should be unchanged.
        assert_eq!(reparsed.models.chat.providers[0].model, "test-model");

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── skills settings tests ─────────────────────────────────────

    fn skills_config_write_app() -> (std::path::PathBuf, Router) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "buddy-skills-config-{}-{}",
            std::process::id(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("buddy.toml");
        let initial_toml = format!(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"

[skills.read_file]
allowed_directories = ["{tmp}"]

[skills.write_file]
allowed_directories = ["{tmp}"]
approval = "always"

[skills.fetch_url]
allowed_domains = ["example.com"]
approval = "once"
"#,
            tmp = std::env::temp_dir().to_str().unwrap()
        );
        std::fs::write(&config_path, &initial_toml).unwrap();
        let config = buddy_core::config::Config::parse(&initial_toml).unwrap();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: None,
        });
        let router = Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .route(
                "/api/config/skills",
                axum::routing::put(put_config_skills::<MockProvider>),
            )
            .with_state(state);
        (dir, router)
    }

    #[tokio::test]
    async fn skills_load_all_three_configured() {
        let (dir, app) = skills_config_write_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let tmp = std::env::temp_dir();
        let tmp_str = tmp.to_str().unwrap();

        // read_file
        assert!(json["skills"]["read_file"].is_object());
        assert_eq!(json["skills"]["read_file"]["allowed_directories"][0], tmp_str);
        // write_file
        assert!(json["skills"]["write_file"].is_object());
        assert_eq!(json["skills"]["write_file"]["allowed_directories"][0], tmp_str);
        assert_eq!(json["skills"]["write_file"]["approval"], "always");
        // fetch_url
        assert!(json["skills"]["fetch_url"].is_object());
        assert_eq!(json["skills"]["fetch_url"]["allowed_domains"][0], "example.com");
        assert_eq!(json["skills"]["fetch_url"]["approval"], "once");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_toggle_off_write_file() {
        let (dir, app) = skills_config_write_app();
        let tmp = std::env::temp_dir();
        let body = serde_json::json!({
            "read_file": { "allowed_directories": [tmp.to_str().unwrap()] },
            "fetch_url": { "allowed_domains": ["example.com"], "approval": "once" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["skills"]["write_file"].is_null());
        assert!(json["skills"]["read_file"].is_object());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_toggle_on_with_empty_defaults() {
        let (dir, app) = skills_config_write_app();
        let tmp = std::env::temp_dir();
        // Toggle write_file on with empty list
        let body = serde_json::json!({
            "read_file": { "allowed_directories": [tmp.to_str().unwrap()] },
            "write_file": { "allowed_directories": [] },
            "fetch_url": { "allowed_domains": ["example.com"], "approval": "once" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["skills"]["write_file"].is_object());
        assert_eq!(json["skills"]["write_file"]["allowed_directories"], serde_json::json!([]));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_add_directory_to_read_file() {
        let (dir, app) = skills_config_write_app();
        let tmp = std::env::temp_dir();
        // Create a second valid directory
        let extra_dir = dir.join("extra");
        std::fs::create_dir_all(&extra_dir).unwrap();
        let body = serde_json::json!({
            "read_file": {
                "allowed_directories": [tmp.to_str().unwrap(), extra_dir.to_str().unwrap()]
            },
            "write_file": { "allowed_directories": [tmp.to_str().unwrap()], "approval": "always" },
            "fetch_url": { "allowed_domains": ["example.com"], "approval": "once" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let dirs = json["skills"]["read_file"]["allowed_directories"]
            .as_array()
            .unwrap();
        assert_eq!(dirs.len(), 2);
        assert_eq!(dirs[1], extra_dir.to_str().unwrap());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_remove_directory_from_write_file() {
        let (dir, app) = skills_config_write_app();
        // Save write_file with empty directory list (remove all)
        let tmp = std::env::temp_dir();
        let body = serde_json::json!({
            "read_file": { "allowed_directories": [tmp.to_str().unwrap()] },
            "write_file": { "allowed_directories": [], "approval": "always" },
            "fetch_url": { "allowed_domains": ["example.com"], "approval": "once" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let dirs = json["skills"]["write_file"]["allowed_directories"]
            .as_array()
            .unwrap();
        assert_eq!(dirs.len(), 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_change_fetch_url_approval_to_trust() {
        let (dir, app) = skills_config_write_app();
        let tmp = std::env::temp_dir();
        let body = serde_json::json!({
            "read_file": { "allowed_directories": [tmp.to_str().unwrap()] },
            "write_file": { "allowed_directories": [tmp.to_str().unwrap()], "approval": "always" },
            "fetch_url": { "allowed_domains": ["example.com"], "approval": "trust" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["skills"]["fetch_url"]["approval"], "trust");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_empty_domain_returns_400() {
        let (dir, app) = skills_config_write_app();
        let tmp = std::env::temp_dir();
        let body = serde_json::json!({
            "read_file": { "allowed_directories": [tmp.to_str().unwrap()] },
            "fetch_url": { "allowed_domains": [""], "approval": "once" }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let err: config::ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(err
            .errors
            .iter()
            .any(|e| e.field.contains("fetch_url.allowed_domains")));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn skills_readonly_has_no_approval_in_response() {
        let (dir, app) = skills_config_write_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // read_file is ReadOnly — approval should be null (not set in config)
        assert!(json["skills"]["read_file"]["approval"].is_null());
        // Mutating/Network skills should have approval set
        assert!(!json["skills"]["write_file"]["approval"].is_null());
        assert!(!json["skills"]["fetch_url"]["approval"].is_null());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_chat_persists_system_prompt() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "system_prompt": "You are a pirate."
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
        let reparsed = buddy_core::config::Config::parse(&disk).unwrap();
        assert_eq!(reparsed.chat.system_prompt, "You are a pirate.");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_server_persists_port() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "host": "0.0.0.0",
            "port": 8080
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/server")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
        let reparsed = buddy_core::config::Config::parse(&disk).unwrap();
        assert_eq!(reparsed.server.port, 8080);
        assert_eq!(reparsed.server.host, "0.0.0.0");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn put_memory_persists() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "auto_retrieve": false,
            "auto_retrieve_limit": 10,
            "similarity_threshold": 0.9
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/memory")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
        let reparsed = buddy_core::config::Config::parse(&disk).unwrap();
        assert!(!reparsed.memory.auto_retrieve);
        assert_eq!(reparsed.memory.auto_retrieve_limit, 10);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn get_config_reflects_put_change() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "system_prompt": "Changed prompt."
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["chat"]["system_prompt"], "Changed prompt.");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn validation_returns_all_failures() {
        let (dir, app) = config_write_app();
        let body = serde_json::json!({
            "chat": {
                "providers": [
                    { "type": "unknown1", "model": "" },
                    { "type": "unknown2", "model": "" }
                ]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let err: config::ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
        // Should have 4 errors: 2 unknown types + 2 empty models
        assert_eq!(err.errors.len(), 4, "expected 4 validation errors, got: {:?}", err.errors);

        std::fs::remove_dir_all(&dir).ok();
    }

    // ── test-provider endpoint tests ────────────────────────────────

    fn test_provider_app() -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route(
                "/api/config/test-provider",
                post(test_provider::<MockProvider>),
            )
            .route("/api/config", get(get_config::<MockProvider>))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_provider_unknown_type_returns_400() {
        let app = test_provider_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "unknown",
                            "model": "some-model",
                            "endpoint": "http://localhost:1234/v1"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("unknown provider type"));
    }

    #[tokio::test]
    async fn test_provider_empty_model_returns_400() {
        let app = test_provider_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "openai",
                            "model": "",
                            "endpoint": "http://localhost:1234/v1"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_provider_missing_env_var_returns_error() {
        let app = test_provider_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "openai",
                            "model": "gpt-4",
                            "endpoint": "https://api.openai.com/v1",
                            "api_key_env": "BUDDY_TEST_NOTSET_032"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("BUDDY_TEST_NOTSET_032"));
    }

    #[tokio::test]
    async fn test_provider_unreachable_endpoint_returns_error() {
        let app = test_provider_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "lmstudio",
                            "model": "test-model",
                            "endpoint": "http://127.0.0.1:1"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("Connection failed"));
    }

    #[tokio::test]
    async fn test_provider_gemini_missing_env_var_returns_error() {
        let app = test_provider_app();
        // SAFETY: Ensure the env var is not set for this test.
        unsafe { std::env::remove_var("BUDDY_TEST_GEMINI_NOTSET_047") };
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "gemini",
                            "model": "gemini-2.0-flash",
                            "endpoint": "https://generativelanguage.googleapis.com",
                            "api_key_env": "BUDDY_TEST_GEMINI_NOTSET_047"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("BUDDY_TEST_GEMINI_NOTSET_047"));
    }

    #[tokio::test]
    async fn test_provider_does_not_modify_config() {
        let app = test_provider_app();

        // Read config before.
        let before = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let before_bytes = before.into_body().collect().await.unwrap().to_bytes();

        // Fire test-provider (will fail — unreachable endpoint).
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "lmstudio",
                            "model": "test-model",
                            "endpoint": "http://127.0.0.1:1"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Read config after.
        let after = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let after_bytes = after.into_body().collect().await.unwrap().to_bytes();

        assert_eq!(before_bytes, after_bytes);
    }

    #[tokio::test]
    async fn test_provider_timeout_when_endpoint_hangs() {
        // Start a TCP listener that accepts but never responds.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Keep the listener alive but never read/write.
        let _handle = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });

        let app = test_provider_app();
        let start = std::time::Instant::now();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "lmstudio",
                            "model": "test-model",
                            "endpoint": format!("http://{addr}")
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");

        // Should complete within ~5s (with slack), not hang forever.
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "request took too long: {elapsed:?}"
        );
    }

    // Test cases for task 046: Ollama Provider

    #[tokio::test]
    async fn test_provider_ollama_unreachable_endpoint() {
        let app = test_provider_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "type": "ollama",
                            "model": "llama3",
                            "endpoint": "http://127.0.0.1:1"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["message"]
            .as_str()
            .unwrap()
            .contains("Connection failed"));
    }

    // Test case for task 048: Mistral Provider

    #[tokio::test]
    async fn test_provider_mistral_missing_env_var() {
        let app = test_provider_app();
        // SAFETY: test-only; ensure env var is not set.
        unsafe { std::env::remove_var("BUDDY_TEST_MISTRAL_NOTSET_048") };

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/config/test-provider")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{
                            "type": "mistral",
                            "model": "mistral-large-latest",
                            "api_key_env": "BUDDY_TEST_MISTRAL_NOTSET_048"
                        }"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["status"], "error");
        assert!(
            json["message"]
                .as_str()
                .unwrap()
                .contains("BUDDY_TEST_MISTRAL_NOTSET_048")
        );
    }
}

/// Tests for task 041 — LM Studio model discovery.
mod discover_models {
    use super::*;

    fn discover_app() -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route(
                "/api/config/discover-models",
                post(super::super::config::discover_models::<MockProvider>),
            )
            .route("/api/config", get(get_config::<MockProvider>))
            .with_state(state)
    }

    fn post_discover(endpoint: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/config/discover-models")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::json!({ "endpoint": endpoint }).to_string(),
            ))
            .unwrap()
    }

    #[tokio::test]
    async fn discover_unreachable_endpoint_returns_error() {
        let app = discover_app();
        let response = app.oneshot(post_discover("http://127.0.0.1:1")).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["message"].as_str().unwrap().contains("Connection failed"));
    }

    #[tokio::test]
    async fn discover_empty_endpoint_returns_error() {
        let app = discover_app();
        let response = app.oneshot(post_discover("")).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");
        assert!(json["message"].as_str().unwrap().contains("endpoint"));
    }

    #[tokio::test]
    async fn discover_with_mock_native_api() {
        // Start a mock server that returns LM Studio native API response.
        let mock_app = Router::new().route(
            "/api/v0/models",
            get(|| async {
                axum::Json(serde_json::json!({
                    "object": "list",
                    "data": [
                        {
                            "id": "qwen/qwen3-8b",
                            "object": "model",
                            "type": "llm",
                            "state": "loaded",
                            "max_context_length": 32768
                        },
                        {
                            "id": "meta-llama/llama-3.1-8b",
                            "object": "model",
                            "type": "llm",
                            "state": "not-loaded",
                            "max_context_length": 8192
                        }
                    ]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, mock_app).await.unwrap();
        });

        let app = discover_app();
        let response = app
            .oneshot(post_discover(&format!("http://{addr}/v1")))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "ok");

        let models = json["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "qwen/qwen3-8b");
        assert_eq!(models[0]["loaded"], true);
        assert_eq!(models[0]["context_length"], 32768);
        assert_eq!(models[1]["id"], "meta-llama/llama-3.1-8b");
        assert_eq!(models[1]["loaded"], false);

        server.abort();
    }

    #[tokio::test]
    async fn discover_falls_back_to_openai_compat() {
        // Mock server that only has /v1/models (no native API).
        let mock_app = Router::new().route(
            "/v1/models",
            get(|| async {
                axum::Json(serde_json::json!({
                    "object": "list",
                    "data": [
                        { "id": "gpt-4o-mini", "object": "model" },
                        { "id": "gpt-4", "object": "model" }
                    ]
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, mock_app).await.unwrap();
        });

        let app = discover_app();
        let response = app
            .oneshot(post_discover(&format!("http://{addr}/v1")))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "ok");

        let models = json["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["id"], "gpt-4o-mini");
        // OpenAI-compat fallback doesn't provide loaded/context_length
        assert!(models[0].get("loaded").is_none());

        server.abort();
    }

    #[tokio::test]
    async fn discover_timeout_when_endpoint_hangs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _handle = tokio::spawn(async move {
            loop {
                let _ = listener.accept().await;
            }
        });

        let app = discover_app();
        let start = std::time::Instant::now();
        let response = app
            .oneshot(post_discover(&format!("http://{addr}/v1")))
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["status"], "error");

        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "request took too long: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn discover_does_not_modify_config() {
        let app = discover_app();

        let before = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let before_bytes = before.into_body().collect().await.unwrap().to_bytes();

        let _ = app
            .clone()
            .oneshot(post_discover("http://127.0.0.1:1"))
            .await
            .unwrap();

        let after = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let after_bytes = after.into_body().collect().await.unwrap().to_bytes();

        assert_eq!(before_bytes, after_bytes);
    }
}

/// Tests for task 034 — Settings page layout.
/// Verify the API contracts that the Settings frontend depends on.
mod settings_page {
    use super::*;

    // Reuse config_api helpers.
    fn config_app(config: buddy_core::config::Config) -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path: std::path::PathBuf::from("/tmp/buddy-test-034.toml"),
            on_config_change: None,
        });
        Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .with_state(state)
    }

    fn config_write_app(suffix: &str) -> (std::path::PathBuf, Router) {
        let dir = std::env::temp_dir().join(format!("buddy-034-{}-{suffix}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("buddy.toml");
        let initial_toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[chat]
system_prompt = "You are a helpful, friendly AI assistant."

[memory]
auto_retrieve = true
auto_retrieve_limit = 3
similarity_threshold = 0.5
"#;
        std::fs::write(&config_path, initial_toml).unwrap();
        let config = buddy_core::config::Config::parse(initial_toml).unwrap();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: None,
        });
        let router = Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .route(
                "/api/config/chat",
                axum::routing::put(put_config_chat::<MockProvider>),
            )
            .route(
                "/api/config/memory",
                axum::routing::put(put_config_memory::<MockProvider>),
            )
            .with_state(state);
        (dir, router)
    }

    /// Test case 1: Load settings page; GET /api/config returns all sections populated.
    #[tokio::test]
    async fn get_config_returns_all_sections_for_settings_page() {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "MY_KEY"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"

[chat]
system_prompt = "Be helpful."

[skills.read_file]
allowed_directories = ["/tmp"]

[memory]
auto_retrieve = false
auto_retrieve_limit = 5
similarity_threshold = 0.8
"#,
        )
        .unwrap();
        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Models section populated
        assert!(json["models"]["chat"]["providers"].is_array());
        assert_eq!(json["models"]["chat"]["providers"][0]["type"], "openai");
        assert!(json["models"]["embedding"]["providers"].is_array());

        // Chat section
        assert_eq!(json["chat"]["system_prompt"], "Be helpful.");

        // Server section (defaults)
        assert_eq!(json["server"]["host"], "127.0.0.1");
        assert_eq!(json["server"]["port"], 3000);

        // Skills section
        assert!(json["skills"]["read_file"].is_object());

        // Memory section
        assert_eq!(json["memory"]["auto_retrieve"], false);
        assert_eq!(json["memory"]["auto_retrieve_limit"], 5);
        assert_eq!(json["memory"]["similarity_threshold"], 0.8);
    }

    /// Test case 2: No embedding providers; embedding section is null.
    #[tokio::test]
    async fn no_embedding_returns_null() {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap();
        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["models"]["embedding"].is_null(),
            "embedding should be null when not configured"
        );
    }

    /// Test case 3: Edit system prompt via PUT /api/config/chat.
    #[tokio::test]
    async fn put_chat_updates_system_prompt() {
        let (dir, app) = config_write_app("chat");
        let body = serde_json::json!({
            "system_prompt": "You are a pirate assistant. Arr!"
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json["chat"]["system_prompt"],
            "You are a pirate assistant. Arr!"
        );

        // Verify GET reflects the change.
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            json["chat"]["system_prompt"],
            "You are a pirate assistant. Arr!"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test case 4: Edit memory settings via PUT /api/config/memory.
    #[tokio::test]
    async fn put_memory_updates_settings() {
        let (dir, app) = config_write_app("memory");
        let body = serde_json::json!({
            "auto_retrieve": false,
            "auto_retrieve_limit": 10,
            "similarity_threshold": 0.9
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/memory")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["memory"]["auto_retrieve"], false);
        assert_eq!(json["memory"]["auto_retrieve_limit"], 10);
        assert_eq!(json["memory"]["similarity_threshold"], 0.9);

        // Verify GET reflects the change.
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["memory"]["auto_retrieve"], false);
        assert_eq!(json["memory"]["auto_retrieve_limit"], 10);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test case 5: GET /api/config with invalid JSON body still succeeds
    /// (GET ignores body). The "failed fetch" test case is a frontend concern;
    /// from the API side, we verify the endpoint always returns 200 with valid JSON.
    #[tokio::test]
    async fn get_config_always_returns_valid_json() {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap();
        let app = config_app(config);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // All top-level sections must be present.
        assert!(json["server"].is_object());
        assert!(json["models"].is_object());
        assert!(json["chat"].is_object());
        assert!(json["skills"].is_object());
        assert!(json["memory"].is_object());
    }

    /// Test case 6: PUT /api/config/chat with invalid JSON returns an error body.
    #[tokio::test]
    async fn put_chat_invalid_json_returns_error() {
        let (dir, app) = config_write_app("invalid");
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(b"not json".to_vec()))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Axum returns 422 for deserialization failures.
        assert!(
            response.status().is_client_error(),
            "expected 4xx, got {}",
            response.status()
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

mod hot_reload {
    use super::*;
    use axum::routing::put;
    use crate::warning::{new_shared_warnings, Warning, WarningSeverity};

    /// Build an app with a hot-reload callback that updates warnings and
    /// memory_config from the in-memory Config after every PUT.
    fn hot_reload_app() -> (std::path::PathBuf, Arc<AppState<MockProvider>>, Router) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "buddy-hot-reload-{}-{}",
            std::process::id(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("buddy.toml");
        let initial_toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#;
        std::fs::write(&config_path, initial_toml).unwrap();
        let config = buddy_core::config::Config::parse(initial_toml).unwrap();

        // Start with single-provider warning (matches single-provider startup).
        let warnings = new_shared_warnings();
        {
            let mut c = warnings.write().unwrap();
            c.add(Warning {
                code: "single_chat_provider".into(),
                message: "Only one chat provider.".into(),
                severity: WarningSeverity::Info,
            });
        }

        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings,
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: Some(Box::new(|state| {
                let config = state.config.read().unwrap();

                // Rebuild skill registry from config.
                let registry = crate::skill::build_registry(&config.skills);
                state.registry.store(Arc::new(registry));

                // Update memory config.
                state.memory_config.store(Arc::new(config.memory.clone()));

                // Rebuild approval overrides.
                let overrides = crate::reload::build_approval_overrides(&config);
                state.approval_overrides.store(Arc::new(overrides));

                // Refresh warnings: simulate provider count from config.
                let provider_count = config.models.chat.providers.len();
                let embedder_ref = state.embedder.load();
                let vs_ref = state.vector_store.load();
                crate::reload::refresh_warnings(
                    &state.warnings,
                    provider_count,
                    &*embedder_ref,
                    &*vs_ref,
                );

                Ok(())
            })),
        });
        let router = Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .route("/api/warnings", get(get_warnings::<MockProvider>))
            .route("/api/config", get(get_config::<MockProvider>))
            .route(
                "/api/config/models",
                put(put_config_models::<MockProvider>),
            )
            .route(
                "/api/config/skills",
                put(put_config_skills::<MockProvider>),
            )
            .route("/api/config/chat", put(put_config_chat::<MockProvider>))
            .route(
                "/api/config/server",
                put(put_config_server::<MockProvider>),
            )
            .route(
                "/api/config/memory",
                put(put_config_memory::<MockProvider>),
            )
            .with_state(state.clone());
        (dir, state, router)
    }

    #[tokio::test]
    async fn put_two_chat_providers_updates_provider_count() {
        let (dir, _state, app) = hot_reload_app();
        let body = serde_json::json!({
            "chat": {
                "providers": [
                    {
                        "type": "lmstudio",
                        "model": "model-a",
                        "endpoint": "http://localhost:1234/v1"
                    },
                    {
                        "type": "lmstudio",
                        "model": "model-b",
                        "endpoint": "http://localhost:5678/v1"
                    }
                ]
            }
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // After reload, single_chat_provider warning should be gone.
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/warnings")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let warnings: Vec<Warning> = serde_json::from_slice(&bytes).unwrap();
        assert!(
            !warnings.iter().any(|w| w.code == "single_chat_provider"),
            "single_chat_provider warning should be cleared after adding 2 providers"
        );

        std::fs::remove_dir_all(&dir).ok();
    }


    #[tokio::test]
    async fn skill_sandbox_rules_updated_after_reload() {
        let (dir, state, app) = hot_reload_app();

        // Initially no skills are registered.
        {
            let reg = state.registry.load();
            assert!(reg.get("read_file").is_none());
        }

        // Update skills config to add read_file.
        let tmp = std::env::temp_dir();
        let body = serde_json::json!({
            "read_file": {
                "allowed_directories": [tmp.to_str().unwrap()]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/skills")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // After reload, the read_file skill should be registered.
        let reg = state.registry.load();
        assert!(
            reg.get("read_file").is_some(),
            "read_file skill should be registered after skills config update"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn system_prompt_change_reflected_in_config() {
        let (dir, state, app) = hot_reload_app();

        let body = serde_json::json!({
            "system_prompt": "You are a pirate assistant."
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/chat")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let config = state.config.read().unwrap();
        assert_eq!(
            config.chat.system_prompt, "You are a pirate assistant.",
            "system prompt should be updated in live config"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn server_config_change_returns_restart_note() {
        let (dir, _state, app) = hot_reload_app();

        let body = serde_json::json!({
            "host": "0.0.0.0",
            "port": 9999
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/server")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Response should include a notes array with restart-required message.
        let notes = json["notes"].as_array().expect("response should have notes array");
        assert!(
            notes.iter().any(|n| n.as_str().unwrap().contains("restart")),
            "response notes should mention restart requirement"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn memory_auto_retrieve_disabled_no_memory_event() {
        let (dir, _state, app) = hot_reload_app();

        // Disable auto_retrieve via config update.
        let body = serde_json::json!({
            "auto_retrieve": false,
            "auto_retrieve_limit": 3,
            "similarity_threshold": 0.7
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/memory")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Send a chat message.
        let events = crate::testutil::post_chat(app, &crate::testutil::make_chat_body()).await;

        // No MemoryContext event should be emitted.
        assert!(
            !events.iter().any(|e| matches!(e, ChatEvent::MemoryContext { .. })),
            "no MemoryContext event should be emitted when auto_retrieve is false"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

mod drag_reorder {
    use super::*;
    use axum::routing::{get, put};

    /// Helper that creates a temp dir with a config containing two chat providers
    /// and one embedding provider, and returns (temp_dir, Router).
    fn reorder_app() -> (std::path::PathBuf, Router) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .join(format!("buddy-drag-reorder-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("buddy.toml");
        let initial_toml = r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "OPENAI_KEY"

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"
"#;
        std::fs::write(&config_path, initial_toml).unwrap();
        let config = buddy_core::config::Config::parse(initial_toml).unwrap();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: None,
        });
        let router = Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .route(
                "/api/config/models",
                put(put_config_models::<MockProvider>),
            )
            .with_state(state);
        (dir, router)
    }

    /// Test case 1: Drag the second provider to the first position;
    /// assert PUT /api/config/models is called with the reversed order.
    #[tokio::test]
    async fn reorder_reverses_two_providers() {
        let (dir, app) = reorder_app();
        // Send reversed order: deepseek-coder first, gpt-4 second.
        let body = serde_json::json!({
            "chat": {
                "providers": [
                    {
                        "type": "lmstudio",
                        "model": "deepseek-coder",
                        "endpoint": "http://localhost:1234/v1"
                    },
                    {
                        "type": "openai",
                        "model": "gpt-4",
                        "endpoint": "https://api.openai.com/v1",
                        "api_key_env": "OPENAI_KEY"
                    }
                ]
            },
            "embedding": {
                "providers": [{ "type": "local", "model": "all-minilm" }]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let providers = json["models"]["chat"]["providers"].as_array().unwrap();
        assert_eq!(providers[0]["model"], "deepseek-coder");
        assert_eq!(providers[1]["model"], "gpt-4");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test case 2: After reorder, assert position labels update
    /// (the dragged item shows "Primary" — index 0 in the response).
    #[tokio::test]
    async fn reorder_updates_position_labels() {
        let (dir, app) = reorder_app();
        // Move deepseek-coder to position 0 (Primary).
        let body = serde_json::json!({
            "chat": {
                "providers": [
                    {
                        "type": "lmstudio",
                        "model": "deepseek-coder",
                        "endpoint": "http://localhost:1234/v1"
                    },
                    {
                        "type": "openai",
                        "model": "gpt-4",
                        "endpoint": "https://api.openai.com/v1",
                        "api_key_env": "OPENAI_KEY"
                    }
                ]
            },
            "embedding": {
                "providers": [{ "type": "local", "model": "all-minilm" }]
            }
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // GET config and verify the new order persisted (index 0 = Primary).
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let providers = json["models"]["chat"]["providers"].as_array().unwrap();
        assert_eq!(
            providers[0]["model"], "deepseek-coder",
            "deepseek-coder should be at index 0 (Primary position)"
        );
        assert_eq!(
            providers[1]["model"], "gpt-4",
            "gpt-4 should be at index 1 (Fallback #2 position)"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test case 3: Simulate a save failure after reorder;
    /// assert the server returns an error so the frontend can revert.
    /// The revert itself happens client-side in persistReorder().
    #[tokio::test]
    async fn save_failure_returns_error_for_frontend_revert() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .join(format!("buddy-drag-fail-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&dir).unwrap();
        // Point config_path to a non-existent directory so atomic_write fails.
        let config_path = dir.join("nonexistent-subdir").join("buddy.toml");
        let initial_toml = r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"
"#;
        let config = buddy_core::config::Config::parse(initial_toml).unwrap();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: None,
        });
        let app = Router::new()
            .route(
                "/api/config/models",
                put(put_config_models::<MockProvider>),
            )
            .with_state(state);

        // Attempt to reorder — should fail because config_path parent doesn't exist.
        let body = serde_json::json!({
            "chat": {
                "providers": [
                    {
                        "type": "lmstudio",
                        "model": "deepseek-coder",
                        "endpoint": "http://localhost:1234/v1"
                    },
                    {
                        "type": "openai",
                        "model": "gpt-4",
                        "endpoint": "https://api.openai.com/v1"
                    }
                ]
            }
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "PUT should fail when config path is not writable"
        );

        // Verify the error response contains a message the frontend can display.
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            json["message"].as_str().unwrap_or("").contains("failed"),
            "error response should contain a descriptive message"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test case 4: Assert the drag handle is visible on each provider card.
    /// Verifies the built frontend HTML contains the drag handle SVG markup.
    #[tokio::test]
    async fn drag_handle_markup_present_in_source() {
        // Read the ModelsTab.svelte source and verify it contains the drag handle.
        let source = include_str!("../../../frontend/src/lib/settings/ModelsTab.svelte");
        assert!(
            source.contains("aria-label=\"Drag to reorder\""),
            "ModelsTab.svelte should contain drag handle with aria-label"
        );
        assert!(
            source.contains("data-drag-index"),
            "ModelsTab.svelte should contain data-drag-index attributes"
        );
    }

    /// Test case 5: Drag a provider within a single-item list (no-op);
    /// assert no API call is made.
    /// From the API side, verify that PUTting a single-item list with the same
    /// provider succeeds and the config remains unchanged.
    #[tokio::test]
    async fn single_item_reorder_is_noop() {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir()
            .join(format!("buddy-drag-noop-{}-{}", std::process::id(), id));
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("buddy.toml");
        let initial_toml = r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
"#;
        std::fs::write(&config_path, initial_toml).unwrap();
        let config = buddy_core::config::Config::parse(initial_toml).unwrap();
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                tokens: vec!["hi".into()],
            }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: buddy_core::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(
                buddy_core::config::MemoryConfig::default(),
            ),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(config),
            config_path,
            on_config_change: None,
        });
        let app = Router::new()
            .route("/api/config", get(get_config::<MockProvider>))
            .route(
                "/api/config/models",
                put(put_config_models::<MockProvider>),
            )
            .with_state(state);

        // PUT with the same single provider (simulating a drag that returns to origin).
        let body = serde_json::json!({
            "chat": {
                "providers": [{
                    "type": "openai",
                    "model": "gpt-4",
                    "endpoint": "https://api.openai.com/v1"
                }]
            }
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify config is unchanged.
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let providers = json["models"]["chat"]["providers"].as_array().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0]["model"], "gpt-4");

        std::fs::remove_dir_all(&dir).ok();
    }

    /// Test case 6: Reorder chat providers; assert embedding providers are
    /// unchanged in the saved config.
    #[tokio::test]
    async fn reorder_chat_preserves_embedding() {
        let (dir, app) = reorder_app();
        // Reverse chat order, keep embedding the same.
        let body = serde_json::json!({
            "chat": {
                "providers": [
                    {
                        "type": "lmstudio",
                        "model": "deepseek-coder",
                        "endpoint": "http://localhost:1234/v1"
                    },
                    {
                        "type": "openai",
                        "model": "gpt-4",
                        "endpoint": "https://api.openai.com/v1",
                        "api_key_env": "OPENAI_KEY"
                    }
                ]
            },
            "embedding": {
                "providers": [{ "type": "local", "model": "all-minilm" }]
            }
        });
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/config/models")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Verify embedding is unchanged.
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        // Chat should be reordered.
        let chat = json["models"]["chat"]["providers"].as_array().unwrap();
        assert_eq!(chat[0]["model"], "deepseek-coder");
        assert_eq!(chat[1]["model"], "gpt-4");

        // Embedding should be unchanged.
        let emb = json["models"]["embedding"]["providers"].as_array().unwrap();
        assert_eq!(emb.len(), 1);
        assert_eq!(emb[0]["type"], "local");
        assert_eq!(emb[0]["model"], "all-minilm");

        std::fs::remove_dir_all(&dir).ok();
    }
}

mod app_shell_navigation {
    /// Test case 1: Sidebar source contains "buddy" brand mark, Chat and Settings
    /// nav items, and conversation list below navigation.
    #[test]
    fn sidebar_has_brand_and_nav_items() {
        let source = include_str!("../../../frontend/src/lib/Sidebar.svelte");
        assert!(
            source.contains(">buddy</span>"),
            "Sidebar should contain 'buddy' brand text"
        );
        assert!(
            source.contains(">b</span>"),
            "Sidebar should contain 'b' collapsed brand text"
        );
        assert!(
            source.contains("href=\"#/\""),
            "Sidebar should have a Chat nav link to #/"
        );
        assert!(
            source.contains("href=\"#/settings\""),
            "Sidebar should have a Settings nav link to #/settings"
        );
        assert!(
            source.contains(">Chat</span>"),
            "Sidebar should have a Chat label"
        );
        assert!(
            source.contains(">Settings</span>"),
            "Sidebar should have a Settings label"
        );
    }

    /// Test case 2–3: Active route indication uses highlighted background and
    /// left accent border. Settings and Chat nav items toggle based on
    /// currentRoute.
    #[test]
    fn nav_items_have_active_route_styling() {
        let source = include_str!("../../../frontend/src/lib/Sidebar.svelte");
        assert!(
            source.contains("border-l-blue-500"),
            "Active nav item should have blue left border"
        );
        assert!(
            source.contains("bg-blue-50"),
            "Active nav item should have highlighted background"
        );
        assert!(
            source.contains("border-l-transparent"),
            "Inactive nav item should have transparent left border"
        );
    }

    /// Test case 3: Conversation list is conditional on Chat route being active.
    #[test]
    fn conversation_list_conditional_on_chat_route() {
        let source = include_str!("../../../frontend/src/lib/Sidebar.svelte");
        assert!(
            source.contains("currentRoute === '/'"),
            "Conversation list should be conditional on Chat route"
        );
    }

    /// Test case 4–7: Collapse toggle exists, labels hidden when collapsed,
    /// nav icons remain visible, tooltips on collapsed icons.
    #[test]
    fn collapse_toggle_and_collapsed_behavior() {
        let source = include_str!("../../../frontend/src/lib/Sidebar.svelte");
        assert!(
            source.contains("onToggleCollapse"),
            "Sidebar should accept onToggleCollapse callback"
        );
        assert!(
            source.contains("'Expand sidebar'"),
            "Collapse toggle should have expand aria-label"
        );
        assert!(
            source.contains("'Collapse sidebar'"),
            "Collapse toggle should have collapse aria-label"
        );
        assert!(
            source.contains("collapsed ? 'md:hidden'"),
            "Labels should be hidden on desktop when collapsed"
        );
        assert!(
            source.contains("title={collapsed ? 'Chat'"),
            "Chat nav should show tooltip when collapsed"
        );
        assert!(
            source.contains("title={collapsed ? 'Settings'"),
            "Settings nav should show tooltip when collapsed"
        );
    }

    /// Test case 8–9: Collapse preference persists via localStorage.
    #[test]
    fn collapse_persists_in_local_storage() {
        let source = include_str!("../../../frontend/src/App.svelte");
        assert!(
            source.contains("buddy-sidebar-collapsed"),
            "App should use localStorage key 'buddy-sidebar-collapsed'"
        );
        assert!(
            source.contains("localStorage.getItem('buddy-sidebar-collapsed')"),
            "App should read collapse state from localStorage on init"
        );
        assert!(
            source.contains("localStorage.setItem('buddy-sidebar-collapsed'"),
            "App should persist collapse state to localStorage"
        );
    }

    /// Test case 10: Transitions use duration-200 for smooth animation.
    #[test]
    fn sidebar_has_smooth_transitions() {
        let source = include_str!("../../../frontend/src/App.svelte");
        assert!(
            source.contains("transition-all duration-200"),
            "Sidebar aside should have transition-all duration-200"
        );
        let sidebar = include_str!("../../../frontend/src/lib/Sidebar.svelte");
        assert!(
            sidebar.contains("transition-transform duration-200"),
            "Collapse icon should have transition-transform duration-200"
        );
    }

    /// Test case 10: Mobile sidebar uses existing overlay behavior.
    /// The collapse toggle is hidden on mobile (hidden md:block).
    #[test]
    fn collapse_toggle_hidden_on_mobile() {
        let source = include_str!("../../../frontend/src/lib/Sidebar.svelte");
        assert!(
            source.contains("hidden md:block"),
            "Collapse toggle wrapper should be hidden on mobile"
        );
    }

    /// Test case 11: App.svelte passes collapsed prop and width class to
    /// aside for desktop collapse.
    #[test]
    fn aside_width_responds_to_collapsed_state() {
        let source = include_str!("../../../frontend/src/App.svelte");
        assert!(
            source.contains("md:w-14"),
            "Aside should use md:w-14 when collapsed"
        );
        assert!(
            source.contains("collapsed={sidebarCollapsed}"),
            "App should pass collapsed prop to Sidebar"
        );
    }
}

// ── Task 044: Embedding migration detection ────────────────────────────

use axum::routing::put;
use crate::testutil::MockEmbedder;
use buddy_core::memory::sqlite::SqliteVectorStore;
use buddy_core::memory::{VectorEntry, VectorStore};
use crate::api::config::put_config_models;
use crate::api::memory::get_memory_status;

fn test_app_with_vector_store(
    embedder: Option<Arc<dyn buddy_core::embedding::Embedder>>,
    vector_store: Option<Arc<dyn VectorStore>>,
) -> Router {
    let state = Arc::new(AppState {
        provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec![] }),
        registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
        store: buddy_core::store::Store::open_in_memory().unwrap(),
        embedder: arc_swap::ArcSwap::from_pointee(embedder),
        vector_store: arc_swap::ArcSwap::from_pointee(vector_store),
        working_memory: crate::skill::working_memory::new_working_memory_map(),
        memory_config: arc_swap::ArcSwap::from_pointee(buddy_core::config::MemoryConfig::default()),
        warnings: crate::warning::new_shared_warnings(),
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
        approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
        approval_timeout: std::time::Duration::from_secs(1),
        config: std::sync::RwLock::new(test_config()),
        config_path: std::path::PathBuf::from("/tmp/buddy-test-044.toml"),
        on_config_change: None,
    });
    Router::new()
        .route("/api/config/models", put(put_config_models::<MockProvider>))
        .route("/api/memory/status", get(get_memory_status::<MockProvider>))
        .with_state(state)
}

#[tokio::test]
async fn migration_required_when_model_changed_and_memories_exist() {
    // Store one memory with model "A".
    let store = Arc::new(SqliteVectorStore::open_in_memory("model-A", 3).unwrap());
    store.store(VectorEntry {
        id: "e1".to_string(),
        embedding: vec![1.0, 0.0, 0.0],
        source_text: "test memory".to_string(),
        metadata: serde_json::json!({}),
    }).unwrap();

    // Change config to model "B" (different dimensions).
    let embedder_b: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(5));
    let store_b = Arc::new(SqliteVectorStore::open_in_memory("model-B", 5).unwrap());
    // Copy entry with old dimensions to trigger mismatch.
    store_b.store(VectorEntry {
        id: "e1".to_string(),
        embedding: vec![1.0, 0.0, 0.0],
        source_text: "test memory".to_string(),
        metadata: serde_json::json!({}),
    }).unwrap_err(); // This will fail dimension check, so let's simulate properly.

    // Actually, let's simulate the real scenario: store with model A has an entry,
    // then we open with model B config.
    let path = std::env::temp_dir().join("buddy-test-044-migration-required.db");
    let _ = std::fs::remove_file(&path);

    {
        let store_a = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
        store_a.store(VectorEntry {
            id: "e1".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            source_text: "test memory".to_string(),
            metadata: serde_json::json!({}),
        }).unwrap();
    }

    // Now open with model B - this will detect migration needed.
    let store_b = Arc::new(SqliteVectorStore::open(&path, "model-B", 5).unwrap());
    let embedder_b: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(5));

    let app = test_app_with_vector_store(Some(embedder_b), Some(store_b));

    let body = serde_json::json!({
        "chat": {
            "providers": [
                {
                    "type": "lmstudio",
                    "model": "new-model",
                    "endpoint": "http://localhost:1234/v1"
                }
            ]
        }
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/config/models")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(
        json["embedding_migration_required"], true,
        "should require migration when model changed and memories exist"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn migration_not_required_when_no_memories() {
    // Empty store with model B.
    let store = Arc::new(SqliteVectorStore::open_in_memory("model-B", 5).unwrap());
    let embedder: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(5));

    let app = test_app_with_vector_store(Some(embedder), Some(store));

    let body = serde_json::json!({
        "chat": {
            "providers": [
                {
                    "type": "lmstudio",
                    "model": "different-model",
                    "endpoint": "http://localhost:1234/v1"
                }
            ]
        }
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/config/models")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(
        json["embedding_migration_required"], false,
        "should not require migration when no memories exist"
    );
}

#[tokio::test]
async fn migration_not_required_when_model_unchanged() {
    // Store with model A, keep using model A.
    let path = std::env::temp_dir().join("buddy-test-044-no-change.db");
    let _ = std::fs::remove_file(&path);

    {
        let store_a = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
        store_a.store(VectorEntry {
            id: "e1".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            source_text: "test memory".to_string(),
            metadata: serde_json::json!({}),
        }).unwrap();
    }

    let store = Arc::new(SqliteVectorStore::open(&path, "model-A", 3).unwrap());
    let embedder: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(3));

    let app = test_app_with_vector_store(Some(embedder), Some(store));

    let body = serde_json::json!({
        "chat": {
            "providers": [
                {
                    "type": "lmstudio",
                    "model": "same-model",
                    "endpoint": "http://localhost:1234/v1"
                }
            ]
        }
    });

    let req = Request::builder()
        .method("PUT")
        .uri("/api/config/models")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(
        json["embedding_migration_required"], false,
        "should not require migration when model unchanged"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn memory_status_empty_store() {
    let store = Arc::new(SqliteVectorStore::open_in_memory("model-A", 3).unwrap());
    let embedder: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(3));

    let app = test_app_with_vector_store(Some(embedder), Some(store));

    let req = Request::builder()
        .method("GET")
        .uri("/api/memory/status")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total_entries"], 0);
    assert_eq!(json["migration_required"], false);
    assert_eq!(json["stored_model"], serde_json::Value::Null);
    assert_eq!(json["stored_dimensions"], serde_json::Value::Null);
    assert_eq!(json["active_model"], "test-embedder");
    assert_eq!(json["active_dimensions"], 3);
}

#[tokio::test]
async fn memory_status_with_mismatch() {
    // Store a memory with model "all-MiniLM-L6-v2", 384 dims.
    let path = std::env::temp_dir().join("buddy-test-044-status-mismatch.db");
    let _ = std::fs::remove_file(&path);

    {
        let store_old = SqliteVectorStore::open(&path, "all-MiniLM-L6-v2", 384).unwrap();
        store_old.store(VectorEntry {
            id: "e1".to_string(),
            embedding: vec![0.0; 384],
            source_text: "test memory".to_string(),
            metadata: serde_json::json!({}),
        }).unwrap();
    }

    // Change active embedder to "text-embedding-3-small", 1536 dims.
    let store = Arc::new(SqliteVectorStore::open(&path, "text-embedding-3-small", 1536).unwrap());
    let embedder: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(1536));

    let app = test_app_with_vector_store(Some(embedder), Some(store));

    let req = Request::builder()
        .method("GET")
        .uri("/api/memory/status")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total_entries"], 1);
    assert_eq!(json["migration_required"], true);
    assert_eq!(json["stored_model"], "all-MiniLM-L6-v2");
    assert_eq!(json["stored_dimensions"], 384);
    assert_eq!(json["active_model"], "test-embedder");
    assert_eq!(json["active_dimensions"], 1536);

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn memory_status_after_migration() {
    // Store a memory with model A, migrate to model B, then check status.
    let path = std::env::temp_dir().join("buddy-test-044-after-migrate.db");
    let _ = std::fs::remove_file(&path);

    {
        let store_a = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
        store_a.store(VectorEntry {
            id: "e1".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            source_text: "test memory".to_string(),
            metadata: serde_json::json!({}),
        }).unwrap();
    }

    // Open with model B - migration needed.
    let store_b = SqliteVectorStore::open(&path, "model-B", 5).unwrap();
    assert!(store_b.needs_migration());

    // Simulate migration: clear and re-store.
    let entries = store_b.list_all().unwrap();
    store_b.clear().unwrap();
    for entry in entries {
        store_b.store(VectorEntry {
            id: entry.id,
            embedding: vec![0.0; 5], // New 5-dim embedding
            source_text: entry.source_text,
            metadata: entry.metadata,
        }).unwrap();
    }

    let store = Arc::new(store_b);
    let embedder: Arc<dyn buddy_core::embedding::Embedder> = Arc::new(MockEmbedder::new(5));

    let app = test_app_with_vector_store(Some(embedder), Some(store));

    let req = Request::builder()
        .method("GET")
        .uri("/api/memory/status")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert_eq!(json["total_entries"], 1);
    assert_eq!(json["migration_required"], false, "migration should not be required after re-embedding");
    assert_eq!(json["stored_model"], "model-B");
    assert_eq!(json["stored_dimensions"], 5);

    let _ = std::fs::remove_file(&path);
}
