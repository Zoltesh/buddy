//! Chat streaming endpoint and tool-call loop.

use std::collections::HashMap;
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
use tokio::sync::oneshot;

use super::{ApiError, AppState, ApproveRequest, ChatEvent, ChatRequest, MemorySnippet};
use buddy_core::config::ApprovalPolicy;
use buddy_core::types::{Message, MessageContent, Role};
use crate::provider::{Provider, Token};
use crate::skill::PermissionLevel;
use buddy_core::store::title_from_message;

/// Maximum number of tool-call loop iterations before aborting.
pub(super) const MAX_TOOL_ITERATIONS: usize = 10;

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
                    MessageContent::Text { text } => Some(title_from_message(&text)),
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
        let registry = state.registry.load();
        let defs = registry.tool_definitions();
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

/// `POST /api/chat/{conversation_id}/approve` — approve or deny a pending skill execution.
pub async fn approve_handler<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(_conversation_id): Path<String>,
    Json(body): Json<ApproveRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let sender = {
        let mut pending = state.pending_approvals.lock().await;
        pending.remove(&body.approval_id)
    };

    match sender {
        Some(tx) => {
            let _ = tx.send(body.approved);
            Ok(StatusCode::OK)
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: "not_found".into(),
                message: format!("approval '{}' not found or already resolved", body.approval_id),
            }),
        )),
    }
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Persist a message to the store, logging errors without crashing.
fn persist_message(store: &buddy_core::store::Store, conversation_id: &str, message: &Message) {
    if let Err(e) = store.append_message(conversation_id, message) {
        eprintln!("warning: failed to persist message: {e}");
    }
}

/// Check whether a skill execution should proceed, applying the approval policy.
///
/// Returns `true` if the skill is approved (by policy, prior approval, or user action),
/// `false` if denied or timed out.
async fn check_approval<P: Provider>(
    state: &Arc<AppState<P>>,
    approval_overrides: &HashMap<String, ApprovalPolicy>,
    tx: &tokio::sync::mpsc::Sender<ChatEvent>,
    conversation_id: &str,
    skill_name: &str,
    arguments: &str,
    permission_level: PermissionLevel,
) -> bool {
    // Resolve effective policy: config override, or default Always for non-ReadOnly.
    let policy = approval_overrides
        .get(skill_name)
        .copied()
        .unwrap_or(ApprovalPolicy::Always);

    match policy {
        ApprovalPolicy::Trust => return true,
        ApprovalPolicy::Once => {
            let approvals = state.conversation_approvals.lock().await;
            if let Some(skills) = approvals.get(conversation_id) {
                if skills.contains(skill_name) {
                    return true;
                }
            }
        }
        ApprovalPolicy::Always => {}
    }

    // Need to ask the user.
    let approval_id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel::<bool>();

    {
        let mut pending = state.pending_approvals.lock().await;
        pending.insert(approval_id.clone(), sender);
    }

    let args_value: serde_json::Value = serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::json!({}));

    let perm_str = match permission_level {
        PermissionLevel::ReadOnly => "read_only",
        PermissionLevel::Mutating => "mutating",
        PermissionLevel::Network => "network",
    };

    let _ = tx
        .send(ChatEvent::ApprovalRequest {
            id: approval_id.clone(),
            skill_name: skill_name.to_string(),
            arguments: args_value,
            permission_level: perm_str.to_string(),
        })
        .await;

    let result = tokio::time::timeout(state.approval_timeout, receiver).await;

    // Cleanup pending entry regardless of outcome.
    {
        let mut pending = state.pending_approvals.lock().await;
        pending.remove(&approval_id);
    }

    match result {
        Ok(Ok(true)) => {
            // Record for `once` policy.
            if policy == ApprovalPolicy::Once {
                let mut approvals = state.conversation_approvals.lock().await;
                approvals
                    .entry(conversation_id.to_string())
                    .or_default()
                    .insert(skill_name.to_string());
            }
            true
        }
        _ => false,
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

    // Load hot-reloadable state snapshots for the duration of this request.
    let memory_config = state.memory_config.load();
    let embedder = state.embedder.load();
    let vector_store = state.vector_store.load();
    let registry = state.registry.load();
    let provider = state.provider.load();
    let approval_overrides = state.approval_overrides.load();

    // Automatic context retrieval: search long-term memory for relevant memories.
    let mut recalled_context: Option<String> = None;
    if memory_config.auto_retrieve
        && !disable_memory
        && embedder.is_some()
        && vector_store.is_some()
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
            let emb = (**embedder).as_ref().unwrap();
            let vs = (**vector_store).as_ref().unwrap();

            if let Ok(embeddings) = emb.embed(&[query_text]) {
                if let Some(embedding) = embeddings.into_iter().next() {
                    if let Ok(results) = vs.search(&embedding, memory_config.auto_retrieve_limit) {
                        let threshold = memory_config.similarity_threshold;
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
        let token_stream = match provider.complete(provider_messages, tools.clone()).await {
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

            // Execute the skill (with approval check for non-ReadOnly skills).
            let result_content = match registry.get(name) {
                Some(skill) => {
                    let perm = skill.permission_level();
                    let approved = if perm == PermissionLevel::ReadOnly {
                        true
                    } else {
                        check_approval(
                            &state, &approval_overrides, &tx, &conversation_id, name, arguments, perm,
                        ).await
                    };

                    if !approved {
                        format!("User denied execution of {name}")
                    } else {
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
