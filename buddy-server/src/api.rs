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
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::Json;
use chrono::Utc;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::provider::{Provider, Token};
use crate::store::title_from_message;
use crate::types::{Message, MessageContent, Role};

/// Maximum number of tool-call loop iterations before aborting.
const MAX_TOOL_ITERATIONS: usize = 10;

/// Incoming chat request.
#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub disable_memory: bool,
}

/// A recalled memory snippet surfaced to the frontend.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct MemorySnippet {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub score: f32,
}

/// A single frame in the streamed response.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    ConversationMeta { conversation_id: String },
    Warnings { warnings: Vec<crate::warning::Warning> },
    Warning { message: String },
    MemoryContext { memories: Vec<MemorySnippet> },
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
    pub store: crate::store::Store,
    pub embedder: Option<std::sync::Arc<dyn crate::embedding::Embedder>>,
    pub vector_store: Option<std::sync::Arc<dyn crate::memory::VectorStore>>,
    pub working_memory: crate::skill::working_memory::WorkingMemoryMap,
    pub memory_config: crate::config::MemoryConfig,
    pub warnings: crate::warning::SharedWarnings,
}

// ── Conversation CRUD handlers ──────────────────────────────────────────

/// `GET /api/conversations` — list all conversations.
pub async fn list_conversations<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<Vec<crate::store::ConversationSummary>>, (StatusCode, Json<ApiError>)> {
    let list = state.store.list_conversations().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    Ok(Json(list))
}

/// `POST /api/conversations` — create a new empty conversation.
pub async fn create_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<(StatusCode, Json<crate::store::Conversation>), (StatusCode, Json<ApiError>)> {
    let conv = state.store.create_conversation("New conversation").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    Ok((StatusCode::CREATED, Json(conv)))
}

/// `GET /api/conversations/:id` — get a single conversation with all messages.
pub async fn get_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(id): Path<String>,
) -> Result<Json<crate::store::Conversation>, (StatusCode, Json<ApiError>)> {
    let conv = state.store.get_conversation(&id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    match conv {
        Some(c) => Ok(Json(c)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: "not_found".into(),
                message: format!("conversation '{id}' not found"),
            }),
        )),
    }
}

/// `DELETE /api/conversations/:id` — delete a conversation and all messages.
pub async fn delete_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let deleted = state.store.delete_conversation(&id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: "not_found".into(),
                message: format!("conversation '{id}' not found"),
            }),
        ))
    }
}

// ── Warnings endpoint ───────────────────────────────────────────────────

/// `GET /api/warnings` — return current system warnings.
pub async fn get_warnings<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<Vec<crate::warning::Warning>> {
    let collector = state.warnings.read().unwrap();
    Json(collector.list().to_vec())
}

// ── Memory management handlers ──────────────────────────────────────────

/// `POST /api/memory/migrate` — re-embed all stored memories using the current model.
pub async fn migrate_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let vector_store = state.vector_store.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;
    let embedder = state.embedder.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_embedder".into(),
                message: "no embedder configured".into(),
            }),
        )
    })?;

    // Read all existing entries.
    let entries = vector_store.list_all().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: format!("failed to list entries: {e}"),
            }),
        )
    })?;

    // Collect source texts for re-embedding.
    let texts: Vec<&str> = entries.iter().map(|e| e.source_text.as_str()).collect();

    let new_embeddings = if texts.is_empty() {
        Vec::new()
    } else {
        embedder.embed(&texts).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "embedding_failed".into(),
                    message: format!("re-embedding failed: {e}"),
                }),
            )
        })?
    };

    // Clear and re-store with new embeddings.
    vector_store.clear().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: format!("failed to clear store: {e}"),
            }),
        )
    })?;

    let count = entries.len();
    for (entry, embedding) in entries.into_iter().zip(new_embeddings) {
        let new_entry = crate::memory::VectorEntry {
            id: entry.id,
            embedding,
            source_text: entry.source_text,
            metadata: entry.metadata,
        };
        vector_store.store(new_entry).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "internal_error".into(),
                    message: format!("failed to store migrated entry: {e}"),
                }),
            )
        })?;
    }

    Ok(Json(serde_json::json!({ "migrated": count })))
}

/// `DELETE /api/memory` — clear all stored memories.
pub async fn clear_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let vector_store = state.vector_store.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;

    vector_store.clear().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: format!("failed to clear memory: {e}"),
            }),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Chat handler ────────────────────────────────────────────────────────

/// `POST /api/chat` — accepts a `ChatRequest` and streams `ChatEvent` frames via SSE.
///
/// Implements the agentic tool-call loop: the LLM can request tool executions,
/// the backend runs them, feeds results back, and loops until a final text
/// response is produced or the iteration limit is reached.
///
/// If `conversation_id` is provided, loads history from that conversation.
/// If omitted/null, auto-creates a new conversation.
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

    // Resolve or create the conversation, loading existing messages when continuing.
    let (conversation_id, existing_messages) = match &request.conversation_id {
        Some(id) => {
            let conv = state.store.get_conversation(id).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        code: "internal_error".into(),
                        message: e,
                    }),
                )
            })?;
            match conv {
                Some(c) => (id.clone(), c.messages),
                None => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ApiError {
                            code: "not_found".into(),
                            message: format!("conversation '{id}' not found"),
                        }),
                    ));
                }
            }
        }
        None => {
            // Auto-create a conversation, titled from the first user message.
            let title = request
                .messages
                .iter()
                .find(|m| matches!(m.content, MessageContent::Text { .. }) && matches!(m.role, Role::User))
                .and_then(|m| match &m.content {
                    MessageContent::Text { text } => Some(title_from_message(text)),
                    _ => None,
                })
                .unwrap_or_else(|| "New conversation".to_string());

            let conv = state.store.create_conversation(&title).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        code: "internal_error".into(),
                        message: e,
                    }),
                )
            })?;
            (conv.id, Vec::new())
        }
    };

    // Combine existing history with new messages for provider context.
    let new_messages = request.messages;
    let mut all_messages = existing_messages;
    let persist_from = all_messages.len();
    all_messages.extend(new_messages);

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

    let conv_id = conversation_id.clone();
    let disable_memory = request.disable_memory;
    tokio::spawn(async move {
        run_tool_loop(state, conv_id, all_messages, persist_from, tools, tx, disable_memory).await;
    });

    let conv_id_for_meta = conversation_id;
    let events = async_stream::stream! {
        // Emit ConversationMeta as the first event.
        yield Ok::<_, Infallible>(
            Event::default().data(serde_json::to_string(&ChatEvent::ConversationMeta {
                conversation_id: conv_id_for_meta,
            }).unwrap())
        );

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

/// Persist a message to the store, logging errors without crashing.
fn persist_message(state: &impl AsRef<crate::store::Store>, conversation_id: &str, message: &Message) {
    let store = state.as_ref();
    if let Err(e) = store.append_message(conversation_id, message) {
        eprintln!("warning: failed to persist message: {e}");
    }
}

/// Run the tool-call loop, sending `ChatEvent`s through `tx`.
///
/// 1. Send messages + tool definitions to the provider.
/// 2. If the provider yields tool calls: execute them via the `SkillRegistry`,
///    append `ToolCall` and `ToolResult` messages, and call the provider again.
/// 3. Repeat until the provider returns only text (no tool calls).
/// 4. Text deltas are streamed to the client as `TokenDelta` events.
/// 5. Stops after `MAX_TOOL_ITERATIONS` to prevent runaway loops.
/// 6. All messages (user, assistant, tool calls, tool results) are persisted.
async fn run_tool_loop<P: Provider>(
    state: Arc<AppState<P>>,
    conversation_id: String,
    mut messages: Vec<Message>,
    persist_from: usize,
    tools: Option<Vec<serde_json::Value>>,
    tx: tokio::sync::mpsc::Sender<ChatEvent>,
    disable_memory: bool,
) {
    // Persist only new incoming messages (existing ones are already in the DB).
    for msg in &messages[persist_from..] {
        persist_message(&state.store, &conversation_id, msg);
    }

    // Emit current warnings at the start of the stream.
    let startup_warnings = {
        let collector = state.warnings.read().unwrap();
        collector.list().to_vec()
    };
    if !startup_warnings.is_empty() {
        let _ = tx.send(ChatEvent::Warnings { warnings: startup_warnings }).await;
    }

    // Automatic context retrieval: search long-term memory for relevant memories.
    let mut recalled_context: Option<String> = None;
    if state.memory_config.auto_retrieve
        && !disable_memory
        && state.embedder.is_some()
        && state.vector_store.is_some()
    {
        // Find the latest user message text.
        let latest_user_text = messages
            .iter()
            .rev()
            .find_map(|m| match (&m.role, &m.content) {
                (Role::User, MessageContent::Text { text }) => Some(text.as_str()),
                _ => None,
            });

        if let Some(query_text) = latest_user_text {
            let embedder = state.embedder.as_ref().unwrap();
            let vs = state.vector_store.as_ref().unwrap();

            if let Ok(embeddings) = embedder.embed(&[query_text]) {
                if let Some(embedding) = embeddings.into_iter().next() {
                    if let Ok(results) = vs.search(&embedding, state.memory_config.auto_retrieve_limit) {
                        let threshold = state.memory_config.similarity_threshold;
                        let relevant: Vec<_> = results
                            .into_iter()
                            .filter(|r| r.score >= threshold)
                            .collect();

                        if !relevant.is_empty() {
                            // Build system prompt section.
                            let mut context_lines = vec!["## Recalled Memories".to_string()];
                            let mut snippets = Vec::new();
                            for r in &relevant {
                                let category = r.metadata.get("category")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                let cat_label = category.as_deref().unwrap_or("general");
                                context_lines.push(format!(
                                    "- \"{}\" ({}, relevance: {:.2})",
                                    r.source_text, cat_label, r.score
                                ));
                                snippets.push(MemorySnippet {
                                    text: r.source_text.clone(),
                                    category,
                                    score: r.score,
                                });
                            }

                            recalled_context = Some(context_lines.join("\n"));
                            let _ = tx.send(ChatEvent::MemoryContext { memories: snippets }).await;
                        }
                    }
                }
            }
        }
    }

    for _iteration in 0..MAX_TOOL_ITERATIONS {
        // Inject recalled long-term memories and working memory as system context.
        let mut provider_messages = messages.clone();
        if let Some(ctx) = &recalled_context {
            provider_messages.insert(
                0,
                Message {
                    role: Role::System,
                    content: MessageContent::Text { text: ctx.clone() },
                    timestamp: Utc::now(),
                },
            );
        }
        {
            let wm_map = state.working_memory.lock().unwrap();
            if let Some(wm) = wm_map.get(&conversation_id) {
                if !wm.is_empty() {
                    provider_messages.insert(
                        0,
                        Message {
                            role: Role::System,
                            content: MessageContent::Text {
                                text: format!(
                                    "[Working Memory]\n{}",
                                    wm.to_context_string()
                                ),
                            },
                            timestamp: Utc::now(),
                        },
                    );
                }
            }
        }

        // Call the provider.
        let token_stream = match state.provider.complete(provider_messages, tools.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(ChatEvent::Error { message: e.to_string() }).await;
                let _ = tx.send(ChatEvent::Done).await;
                return;
            }
        };

        // Consume the stream, collecting text and tool calls.
        let mut tool_calls: Vec<(String, String, String)> = Vec::new();
        let mut full_text = String::new();

        tokio::pin!(token_stream);
        while let Some(result) = token_stream.next().await {
            match result {
                Ok(Token::Text { text }) => {
                    full_text.push_str(&text);
                    // Stream text deltas immediately.
                    let _ = tx
                        .send(ChatEvent::TokenDelta {
                            content: text,
                        })
                        .await;
                }
                Ok(Token::Warning { message }) => {
                    let _ = tx.send(ChatEvent::Warning { message }).await;
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
            // Final text response — persist and done.
            if !full_text.is_empty() {
                let assistant_msg = Message {
                    role: Role::Assistant,
                    content: MessageContent::Text { text: full_text },
                    timestamp: Utc::now(),
                };
                persist_message(&state.store, &conversation_id, &assistant_msg);
            }
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
            let tool_call_msg = Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                timestamp: Utc::now(),
            };
            persist_message(&state.store, &conversation_id, &tool_call_msg);
            messages.push(tool_call_msg);

            // Execute the skill.
            let result_content = match state.registry.get(name) {
                Some(skill) => {
                    let mut input: serde_json::Value = serde_json::from_str(arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    // Inject conversation context so skills can access per-conversation state.
                    if let Some(obj) = input.as_object_mut() {
                        obj.insert(
                            "conversation_id".to_string(),
                            serde_json::Value::String(conversation_id.clone()),
                        );
                    }
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
            let tool_result_msg = Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: id.clone(),
                    content: result_content,
                },
                timestamp: Utc::now(),
            };
            persist_message(&state.store, &conversation_id, &tool_result_msg);
            messages.push(tool_result_msg);
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

impl AsRef<crate::store::Store> for crate::store::Store {
    fn as_ref(&self) -> &crate::store::Store {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use axum::Router;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use tower_http::services::ServeDir;

    use crate::provider::ProviderChain;
    use crate::skill::SkillRegistry;
    use crate::testutil::{
        ConfigurableMockProvider, FailingSkill, MockEchoSkill, MockProvider, MockResponse,
        SequencedProvider, make_chat_body, make_chat_body_with_conversation, post_chat,
        post_chat_raw,
    };

    // ── Helpers ─────────────────────────────────────────────────────────

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
            provider: MockProvider { tokens },
            registry: SkillRegistry::new(),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: None,
            vector_store: None,
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: crate::config::MemoryConfig::default(),
            warnings: crate::warning::new_shared_warnings(),
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
    }

    fn test_app_with_static(tokens: Vec<String>, static_dir: &str) -> Router {
        let state = Arc::new(AppState {
            provider: MockProvider { tokens },
            registry: SkillRegistry::new(),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: None,
            vector_store: None,
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: crate::config::MemoryConfig::default(),
            warnings: crate::warning::new_shared_warnings(),
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
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: None,
            vector_store: None,
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: crate::config::MemoryConfig::default(),
            warnings: crate::warning::new_shared_warnings(),
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<SequencedProvider>))
            .with_state(state)
    }

    fn conversation_app(tokens: Vec<String>) -> (Arc<AppState<MockProvider>>, Router) {
        let state = Arc::new(AppState {
            provider: MockProvider { tokens },
            registry: SkillRegistry::new(),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: None,
            vector_store: None,
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: crate::config::MemoryConfig::default(),
            warnings: crate::warning::new_shared_warnings(),
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
                provider: chain,
                registry: SkillRegistry::new(),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: None,
                vector_store: None,
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: crate::config::MemoryConfig::default(),
                warnings: crate::warning::new_shared_warnings(),
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
                provider: SequencedProvider::new(vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "echo".into(),
                        r#"{"value":"test"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Final answer.".into()]),
                ]),
                registry: registry_with_echo(),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: None,
                vector_store: None,
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: crate::config::MemoryConfig::default(),
                warnings: crate::warning::new_shared_warnings(),
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
                provider: MockProvider { tokens },
                registry: SkillRegistry::new(),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: None,
                vector_store: None,
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: crate::config::MemoryConfig::default(),
                warnings,
            });
            Router::new()
                .route("/api/chat", post(chat_handler::<MockProvider>))
                .route("/api/warnings", get(get_warnings::<MockProvider>))
                .with_state(state)
        }

        #[tokio::test]
        async fn no_embedding_warning_present() {
            let app = warnings_app(vec![], |c| {
                c.add(Warning {
                    code: "no_embedding_model".into(),
                    message: "No embedding model configured — memory features are disabled. Add a [models.embedding] section to buddy.toml.".into(),
                    severity: WarningSeverity::Warning,
                });
            });

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].code, "no_embedding_model");
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
                provider: MockProvider { tokens: vec![] },
                registry: SkillRegistry::new(),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: None,
                vector_store: None,
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: crate::config::MemoryConfig::default(),
                warnings: warnings.clone(),
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
                provider: MockProvider { tokens: vec![] },
                registry: SkillRegistry::new(),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: None,
                vector_store: None,
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: crate::config::MemoryConfig::default(),
                warnings: warnings.clone(),
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
                    code: "no_embedding_model".into(),
                    message: "No embedding model configured — memory features are disabled. Add a [models.embedding] section to buddy.toml.".into(),
                    severity: WarningSeverity::Warning,
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
    }
}
