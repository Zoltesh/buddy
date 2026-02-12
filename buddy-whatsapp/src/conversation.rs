//! Conversation processing for WhatsApp.
//!
//! Receives a user message, resolves the conversation, calls the provider
//! chain with a tool loop, handles skill approval via interactive buttons,
//! and returns the response text.

use std::collections::HashMap;
use std::time::Duration;

use buddy_core::config::ApprovalPolicy;
use buddy_core::provider::{Provider, ProviderError, Token};
use buddy_core::skill::{PermissionLevel, SkillRegistry};
use buddy_core::state::ConversationApprovals;
use buddy_core::store::Store;
use buddy_core::types::{Message, MessageContent, Role};
use chrono::Utc;
use futures_util::StreamExt;

use crate::approval::WhatsAppPendingApprovals;
use crate::client::WhatsAppClient;

/// Maximum number of tool-call loop iterations (same as web and Telegram).
const MAX_TOOL_ITERATIONS: usize = 10;

/// Maximum length for tool result text before truncation.
const RESULT_MAX_LEN: usize = 2000;

/// Outcome of processing a WhatsApp message.
pub enum ProcessResult {
    /// Final text and any tool results to send (in order): tool results first, then final text.
    Response {
        final_text: String,
        tool_results: Vec<String>,
    },
    /// The provider produced no output (empty stream, no tool calls).
    Empty,
}

/// Errors that can occur during message processing.
pub enum ProcessError {
    /// All configured providers are unavailable (network/rate-limit).
    AllUnavailable,
    /// A provider returned a non-transient error.
    Provider(String),
    /// A database operation failed.
    Store(String),
}

impl ProcessError {
    /// Return a user-facing message suitable for sending via WhatsApp.
    pub fn user_message(&self) -> &str {
        match self {
            Self::AllUnavailable => {
                "All models are currently unavailable. Please check your buddy configuration."
            }
            Self::Provider(_) | Self::Store(_) => {
                "Sorry, I couldn't process that. Please try again."
            }
        }
    }
}

impl std::fmt::Debug for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AllUnavailable => write!(f, "AllUnavailable"),
            Self::Provider(msg) => write!(f, "Provider({msg:?})"),
            Self::Store(msg) => write!(f, "Store({msg:?})"),
        }
    }
}

/// Context for requesting skill approval over WhatsApp.
pub struct WhatsAppApprovalContext<'a> {
    pub client: &'a WhatsAppClient,
    pub phone: &'a str,
    pub pending: &'a WhatsAppPendingApprovals,
    pub timeout: Duration,
}

/// Process a WhatsApp text message: resolve conversation, run provider with tool loop,
/// apply approval policy, persist all messages, return final response text.
pub async fn process_message<P: Provider>(
    store: &Store,
    provider: &P,
    registry: &SkillRegistry,
    approval_overrides: &HashMap<String, ApprovalPolicy>,
    conversation_approvals: &ConversationApprovals,
    phone: &str,
    user_text: &str,
    approval_ctx: Option<WhatsAppApprovalContext<'_>>,
) -> Result<ProcessResult, ProcessError> {
    let conversation_id = resolve_conversation(store, phone, user_text)?;

    let user_msg = Message {
        role: Role::User,
        content: MessageContent::Text {
            text: user_text.to_string(),
        },
        timestamp: Utc::now(),
    };
    store
        .append_message(&conversation_id, &user_msg)
        .map_err(|e| {
            log::error!("Failed to append user message: {e}");
            ProcessError::Store(e)
        })?;

    let messages = match store.get_conversation(&conversation_id) {
        Ok(Some(conv)) => conv.messages,
        Ok(None) => vec![user_msg.clone()],
        Err(e) => {
            log::error!("Failed to load conversation: {e}");
            vec![user_msg.clone()]
        }
    };

    let tools = {
        let defs = registry.tool_definitions();
        if defs.is_empty() {
            None
        } else {
            Some(defs)
        }
    };

    let mut tool_results_to_send: Vec<String> = Vec::new();
    let mut all_messages = messages;
    for _iteration in 0..MAX_TOOL_ITERATIONS {
        let token_stream = match provider
            .complete(all_messages.clone(), tools.clone())
            .await
        {
            Ok(s) => s,
            Err(e) => {
                log::error!("Provider error: {e}");
                return Err(classify_provider_error(e));
            }
        };

        let mut tool_calls: Vec<(String, String, String)> = Vec::new();
        let mut full_text = String::new();

        tokio::pin!(token_stream);
        while let Some(result) = token_stream.next().await {
            match result {
                Ok(Token::Text { text }) => full_text.push_str(&text),
                Ok(Token::Warning { message }) => {
                    log::warn!("Provider warning: {message}");
                }
                Ok(Token::ToolCall {
                    id,
                    name,
                    arguments,
                }) => tool_calls.push((id, name, arguments)),
                Err(e) => {
                    log::error!("Stream error: {e}");
                    return Err(classify_provider_error(e));
                }
            }
        }

        if tool_calls.is_empty() {
            if full_text.is_empty() {
                return Ok(ProcessResult::Empty);
            }
            let assistant_msg = Message {
                role: Role::Assistant,
                content: MessageContent::Text {
                    text: full_text.clone(),
                },
                timestamp: Utc::now(),
            };
            if let Err(e) = store.append_message(&conversation_id, &assistant_msg) {
                log::error!("Failed to append assistant message: {e}");
            }
            return Ok(ProcessResult::Response {
                final_text: full_text,
                tool_results: tool_results_to_send,
            });
        }

        for (id, name, arguments) in &tool_calls {
            let tool_call_msg = Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                timestamp: Utc::now(),
            };
            persist_message(store, &conversation_id, &tool_call_msg);
            all_messages.push(tool_call_msg);

            let result_content = match registry.get(name) {
                Some(skill) => {
                    let perm = skill.permission_level();
                    let approved = if perm == PermissionLevel::ReadOnly {
                        true
                    } else {
                        check_approval_whatsapp(
                            approval_overrides,
                            conversation_approvals,
                            &conversation_id,
                            name,
                            approval_ctx.as_ref(),
                            arguments,
                        )
                        .await
                    };

                    if !approved {
                        format!("User denied execution of {name}")
                    } else {
                        let policy = approval_overrides
                            .get(name)
                            .copied()
                            .unwrap_or(ApprovalPolicy::Always);
                        if policy == ApprovalPolicy::Once {
                            let mut approvals = conversation_approvals.lock().await;
                            approvals
                                .entry(conversation_id.clone())
                                .or_default()
                                .insert(name.clone());
                        }
                        let mut input: serde_json::Value =
                            serde_json::from_str(arguments)
                                .unwrap_or_else(|_| serde_json::json!({}));
                        if let Some(obj) = input.as_object_mut() {
                            obj.insert(
                                "conversation_id".to_string(),
                                serde_json::Value::String(conversation_id.clone()),
                            );
                        }
                        match skill.execute(input).await {
                            Ok(output) => serde_json::to_string(&output)
                                .unwrap_or_else(|_| "{}".to_string()),
                            Err(e) => format!("Tool error: {e}"),
                        }
                    }
                }
                None => "Tool error: unknown tool".to_string(),
            };

            if approval_ctx.is_some() {
                tool_results_to_send.push(format_tool_result(&result_content));
            }

            let tool_result_msg = Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: id.clone(),
                    content: result_content.clone(),
                },
                timestamp: Utc::now(),
            };
            persist_message(store, &conversation_id, &tool_result_msg);
            all_messages.push(tool_result_msg);
        }
    }

    // Exceeded max iterations — make one final completion without tool calls.
    let mut full_text = String::new();
    let token_stream = match provider.complete(all_messages, tools).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Provider error: {e}");
            return Err(classify_provider_error(e));
        }
    };
    tokio::pin!(token_stream);
    while let Some(result) = token_stream.next().await {
        match result {
            Ok(Token::Text { text }) => full_text.push_str(&text),
            Ok(Token::Warning { message }) => log::warn!("Provider warning: {message}"),
            Ok(Token::ToolCall { .. }) => {}
            Err(e) => return Err(classify_provider_error(e)),
        }
    }
    if full_text.is_empty() {
        full_text = "Tool loop reached maximum iterations.".to_string();
    }
    let assistant_msg = Message {
        role: Role::Assistant,
        content: MessageContent::Text {
            text: full_text.clone(),
        },
        timestamp: Utc::now(),
    };
    if let Err(e) = store.append_message(&conversation_id, &assistant_msg) {
        log::error!("Failed to append assistant message: {e}");
    }
    Ok(ProcessResult::Response {
        final_text: full_text,
        tool_results: tool_results_to_send,
    })
}

/// Format tool result for WhatsApp: truncate at RESULT_MAX_LEN.
pub fn format_tool_result(content: &str) -> String {
    if content.len() > RESULT_MAX_LEN {
        let mut end = RESULT_MAX_LEN;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}... (truncated)", &content[..end])
    } else {
        content.to_string()
    }
}

fn persist_message(store: &Store, conversation_id: &str, message: &Message) {
    if let Err(e) = store.append_message(conversation_id, message) {
        log::error!("Failed to persist message: {e}");
    }
}

async fn check_approval_whatsapp(
    approval_overrides: &HashMap<String, ApprovalPolicy>,
    conversation_approvals: &ConversationApprovals,
    conversation_id: &str,
    skill_name: &str,
    approval_ctx: Option<&WhatsAppApprovalContext<'_>>,
    arguments: &str,
) -> bool {
    let policy = approval_overrides
        .get(skill_name)
        .copied()
        .unwrap_or(ApprovalPolicy::Always);

    match policy {
        ApprovalPolicy::Trust => return true,
        ApprovalPolicy::Once => {
            let approvals = conversation_approvals.lock().await;
            if let Some(skills) = approvals.get(conversation_id) {
                if skills.contains(skill_name) {
                    return true;
                }
            }
        }
        ApprovalPolicy::Always => {}
    }

    let Some(ctx) = approval_ctx else {
        return false;
    };
    crate::approval::request_approval(
        ctx.client,
        ctx.phone,
        ctx.pending,
        ctx.timeout,
        skill_name,
        arguments,
    )
    .await
}

/// Look up or create a buddy conversation for a WhatsApp phone number.
fn resolve_conversation(
    store: &Store,
    phone: &str,
    first_message_text: &str,
) -> Result<String, ProcessError> {
    match store.get_conversation_id_for_whatsapp_phone(phone) {
        Ok(Some(id)) => Ok(id),
        Ok(None) => {
            let title: String = first_message_text.trim().chars().take(50).collect();
            let conv = store
                .create_conversation_with_source(&title, "whatsapp")
                .map_err(ProcessError::Store)?;
            store
                .set_whatsapp_chat_mapping(phone, &conv.id)
                .map_err(ProcessError::Store)?;
            Ok(conv.id)
        }
        Err(e) => Err(ProcessError::Store(e)),
    }
}

fn classify_provider_error(e: ProviderError) -> ProcessError {
    match e {
        ProviderError::Network(_) | ProviderError::RateLimit(_) => ProcessError::AllUnavailable,
        _ => ProcessError::Provider(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use buddy_core::config::ApprovalPolicy;
    use buddy_core::skill::SkillRegistry;
    use buddy_core::testutil::{
        MockEchoSkill, MockMutatingSkill, MockNetworkSkill, MockProvider, MockResponse,
        SequencedProvider,
    };

    fn registry_with_echo() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(MockEchoSkill));
        r
    }

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

    #[tokio::test]
    async fn new_sender_creates_whatsapp_conversation() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["Hello!".into()],
        };
        let registry = SkillRegistry::new();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15559876543",
            "Hi there",
            None,
        )
        .await;
        assert!(matches!(result, Ok(ProcessResult::Response { .. })));

        let convs = store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].source, "whatsapp");

        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        assert!(conv.messages.len() >= 2);
        assert_eq!(conv.messages[0].role, Role::User);
        assert!(matches!(
            &conv.messages[0].content,
            MessageContent::Text { text } if text == "Hi there"
        ));
    }

    #[tokio::test]
    async fn provider_called_and_response_returned() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["I can ".into(), "help!".into()],
        };
        let registry = SkillRegistry::new();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "Help me",
            None,
        )
        .await;
        let response = match result {
            Ok(ProcessResult::Response { final_text, .. }) => final_text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert_eq!(response, "I can help!");

        let convs = store.list_conversations().unwrap();
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        let last = conv.messages.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
        assert!(matches!(
            &last.content,
            MessageContent::Text { text } if text == "I can help!"
        ));
    }

    #[tokio::test]
    async fn same_sender_reuses_conversation() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["ok".into()],
        };
        let registry = SkillRegistry::new();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15559876543",
            "First",
            None,
        )
        .await
        .unwrap();
        process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15559876543",
            "Second",
            None,
        )
        .await
        .unwrap();

        let convs = store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        assert_eq!(conv.messages.len(), 4); // 2 user + 2 assistant
    }

    /// ReadOnly skill (echo) executes without approval; no approval context needed.
    #[tokio::test]
    async fn readonly_skill_executes_without_approval() {
        let store = Store::open_in_memory().unwrap();
        let provider = SequencedProvider::new(vec![
            MockResponse::ToolCalls(vec![(
                "c1".into(),
                "echo".into(),
                r#"{"value":"hello"}"#.into(),
            )]),
            MockResponse::Text(vec!["Done.".into()]),
        ]);
        let registry = registry_with_echo();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "Use echo",
            None,
        )
        .await;
        let response = match result {
            Ok(ProcessResult::Response { final_text, .. }) => final_text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert!(response.contains("Done."));
        let conv = store.list_conversations().unwrap();
        let conv = store.get_conversation(&conv[0].id).unwrap().unwrap();
        let has_tool_result = conv.messages.iter().any(|m| {
            matches!(
                &m.content,
                MessageContent::ToolResult { content, .. } if content.contains("hello")
            )
        });
        assert!(has_tool_result, "ToolCall and ToolResult should be stored");
    }

    /// Network skill with Trust policy executes without approval.
    #[tokio::test]
    async fn network_skill_with_trust_policy_executes_without_approval() {
        let store = Store::open_in_memory().unwrap();
        let provider = SequencedProvider::new(vec![
            MockResponse::ToolCalls(vec![(
                "c1".into(),
                "network".into(),
                r#"{"value":"ok"}"#.into(),
            )]),
            MockResponse::Text(vec!["Done.".into()]),
        ]);
        let registry = registry_with_network();
        let mut overrides = HashMap::new();
        overrides.insert("network".to_string(), ApprovalPolicy::Trust);
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "Use network",
            None,
        )
        .await;
        let response = match result {
            Ok(ProcessResult::Response { final_text, .. }) => final_text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert!(response.contains("Done."));
        let conv = store.list_conversations().unwrap();
        let conv = store.get_conversation(&conv[0].id).unwrap().unwrap();
        let has_echo_result = conv.messages.iter().any(|m| {
            matches!(
                &m.content,
                MessageContent::ToolResult { content, .. } if content.contains("echo")
            )
        });
        assert!(
            has_echo_result,
            "Trust policy should execute without approval"
        );
    }

    /// Mutating skill with Always policy (default) and no approval context is denied.
    #[tokio::test]
    async fn mutating_skill_without_approval_context_denied() {
        let store = Store::open_in_memory().unwrap();
        let provider = SequencedProvider::new(vec![
            MockResponse::ToolCalls(vec![(
                "c1".into(),
                "mutating".into(),
                r#"{"value":"x"}"#.into(),
            )]),
            MockResponse::Text(vec!["Denied.".into()]),
        ]);
        let registry = registry_with_mutating();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "Do something",
            None,
        )
        .await;
        let response = match result {
            Ok(ProcessResult::Response { final_text, .. }) => final_text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert!(response.contains("Denied."));
        let conv = store.list_conversations().unwrap();
        let conv = store.get_conversation(&conv[0].id).unwrap().unwrap();
        let has_denied_result = conv.messages.iter().any(|m| {
            matches!(
                &m.content,
                MessageContent::ToolResult { content, .. }
                    if content.contains("User denied execution of mutating")
            )
        });
        assert!(has_denied_result);
    }

    /// Once policy auto-approves when the skill was already approved for this conversation.
    #[tokio::test]
    async fn once_policy_auto_approves_after_prior_approval() {
        let store = Store::open_in_memory().unwrap();
        let mut overrides = HashMap::new();
        overrides.insert("mutating".to_string(), ApprovalPolicy::Once);
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        // Establish conversation with a plain text response.
        let setup_provider = MockProvider {
            tokens: vec!["ok".into()],
        };
        let registry = registry_with_mutating();
        process_message(
            &store,
            &setup_provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "hello",
            None,
        )
        .await
        .unwrap();

        // Pre-populate approval (simulating a prior approved request).
        let conv_id = store.list_conversations().unwrap()[0].id.clone();
        {
            let mut approvals = conversation_approvals.lock().await;
            approvals
                .entry(conv_id.clone())
                .or_default()
                .insert("mutating".to_string());
        }

        // Tool call should auto-approve via Once (no approval context needed).
        let provider = SequencedProvider::new(vec![
            MockResponse::ToolCalls(vec![(
                "c1".into(),
                "mutating".into(),
                r#"{"value":"test"}"#.into(),
            )]),
            MockResponse::Text(vec!["Done.".into()]),
        ]);
        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "mutate something",
            None,
        )
        .await;

        let response = match result {
            Ok(ProcessResult::Response { final_text, .. }) => final_text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert!(response.contains("Done."));

        let conv = store.get_conversation(&conv_id).unwrap().unwrap();
        let executed = conv.messages.iter().any(|m| {
            matches!(
                &m.content,
                MessageContent::ToolResult { content, .. } if content.contains("echo")
            )
        });
        let denied = conv.messages.iter().any(|m| {
            matches!(
                &m.content,
                MessageContent::ToolResult { content, .. } if content.contains("denied")
            )
        });
        assert!(
            executed,
            "Once policy should auto-approve when already approved"
        );
        assert!(!denied);
    }

    /// Tool loop stops after MAX_TOOL_ITERATIONS.
    #[tokio::test]
    async fn tool_loop_stops_at_max_iterations() {
        let store = Store::open_in_memory().unwrap();
        let mut responses = Vec::new();
        for _ in 0..MAX_TOOL_ITERATIONS {
            responses.push(MockResponse::ToolCalls(vec![(
                "c".into(),
                "echo".into(),
                r#"{"value":"x"}"#.into(),
            )]));
        }
        responses.push(MockResponse::Text(vec!["Final.".into()]));
        let provider = SequencedProvider::new(responses);
        let registry = registry_with_echo();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let result = process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "Loop",
            None,
        )
        .await;
        let response = match result {
            Ok(ProcessResult::Response { final_text, .. }) => final_text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert!(
            response.contains("maximum iterations") || response.contains("Final."),
            "expected max iterations message or final text, got {response}"
        );
    }

    /// ToolCall and ToolResult messages are stored in the conversation database.
    #[tokio::test]
    async fn tool_call_and_result_stored_in_database() {
        let store = Store::open_in_memory().unwrap();
        let provider = SequencedProvider::new(vec![
            MockResponse::ToolCalls(vec![(
                "c1".into(),
                "echo".into(),
                r#"{"value":"stored"}"#.into(),
            )]),
            MockResponse::Text(vec!["Done.".into()]),
        ]);
        let registry = registry_with_echo();
        let overrides = HashMap::new();
        let conversation_approvals = Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        process_message(
            &store,
            &provider,
            &registry,
            &overrides,
            &conversation_approvals,
            "15551234567",
            "test storage",
            None,
        )
        .await
        .unwrap();

        let convs = store.list_conversations().unwrap();
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();

        let has_tool_call = conv.messages.iter().any(|m| {
            matches!(&m.content, MessageContent::ToolCall { name, .. } if name == "echo")
        });
        let has_tool_result = conv.messages.iter().any(|m| {
            matches!(&m.content, MessageContent::ToolResult { content, .. } if content.contains("stored"))
        });
        assert!(has_tool_call, "ToolCall message should be stored");
        assert!(has_tool_result, "ToolResult message should be stored");
    }

    #[test]
    fn format_tool_result_truncates() {
        let long = "x".repeat(RESULT_MAX_LEN + 100);
        let out = format_tool_result(&long);
        assert!(out.contains("(truncated)"));
        assert!(out.len() <= RESULT_MAX_LEN + 20);
    }

    #[test]
    fn format_tool_result_keeps_error_prefix() {
        let err = "Tool error: something failed";
        let out = format_tool_result(err);
        assert!(out.contains("Tool error"));
    }

    impl std::fmt::Debug for ProcessResult {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Response {
                    final_text,
                    tool_results,
                } => {
                    write!(
                        f,
                        "Response(final_text: {final_text:?}, {} results)",
                        tool_results.len()
                    )
                }
                Self::Empty => write!(f, "Empty"),
            }
        }
    }
}
