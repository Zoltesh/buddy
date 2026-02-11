use buddy_core::provider::{Provider, ProviderError, Token};
use buddy_core::store::Store;
use buddy_core::types::{Message, MessageContent, Role};
use chrono::Utc;
use futures_util::StreamExt;

/// Outcome of processing a Telegram message.
pub enum ProcessResult {
    /// A text response to send back to the user.
    Response(String),
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
    /// Return a user-facing message suitable for sending via Telegram.
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

/// Process a Telegram text message through the conversation flow.
///
/// 1. Look up or create a conversation for the given `chat_id`.
/// 2. Append the user message.
/// 3. Load conversation history.
/// 4. Call the provider.
/// 5. Consume the token stream, collecting text and tool call names.
/// 6. Decline tool calls with an informational message.
/// 7. Persist the assistant response.
/// 8. Return the response text.
pub async fn process_message<P: Provider>(
    store: &Store,
    provider: &P,
    chat_id: i64,
    user_text: &str,
) -> Result<ProcessResult, ProcessError> {
    // 1. Look up or create conversation.
    let conversation_id = resolve_conversation(store, chat_id, user_text)?;

    // 2. Append user message.
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

    // 3. Load conversation history.
    let messages = match store.get_conversation(&conversation_id) {
        Ok(Some(conv)) => conv.messages,
        Ok(None) => vec![user_msg],
        Err(e) => {
            log::error!("Failed to load conversation: {e}");
            vec![user_msg]
        }
    };

    // 4. Call the provider (no tools — execution is deferred to a future task).
    let token_stream = match provider.complete(messages, None).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("Provider error: {e}");
            return Err(classify_provider_error(e));
        }
    };

    // 5. Consume token stream.
    let mut full_text = String::new();
    let mut tool_call_names: Vec<String> = Vec::new();

    tokio::pin!(token_stream);
    while let Some(result) = token_stream.next().await {
        match result {
            Ok(Token::Text { text }) => full_text.push_str(&text),
            Ok(Token::ToolCall { name, .. }) => tool_call_names.push(name),
            Ok(Token::Warning { message }) => {
                log::warn!("Provider warning: {message}");
            }
            Err(e) => {
                log::error!("Stream error: {e}");
                return Err(classify_provider_error(e));
            }
        }
    }

    // 6. Decline tool calls with informational messages.
    for name in &tool_call_names {
        if !full_text.is_empty() {
            full_text.push_str("\n\n");
        }
        full_text.push_str(&format!(
            "I wanted to use a tool ({name}), but tool execution is not yet available over Telegram."
        ));
    }

    if full_text.is_empty() {
        return Ok(ProcessResult::Empty);
    }

    // 7. Persist assistant response.
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

    Ok(ProcessResult::Response(full_text))
}

/// Look up or create a buddy conversation for a Telegram chat.
fn resolve_conversation(
    store: &Store,
    chat_id: i64,
    first_message_text: &str,
) -> Result<String, ProcessError> {
    match store.get_conversation_id_for_telegram_chat(chat_id) {
        Ok(Some(id)) => Ok(id),
        Ok(None) => {
            let title: String = first_message_text.trim().chars().take(50).collect();
            let conv = store
                .create_conversation_with_source(&title, "telegram")
                .map_err(|e| ProcessError::Store(e))?;
            store
                .set_telegram_chat_mapping(chat_id, &conv.id)
                .map_err(|e| ProcessError::Store(e))?;
            Ok(conv.id)
        }
        Err(e) => Err(ProcessError::Store(e)),
    }
}

/// Map a `ProviderError` to a `ProcessError`.
fn classify_provider_error(e: ProviderError) -> ProcessError {
    match e {
        ProviderError::Network(_) | ProviderError::RateLimit(_) => ProcessError::AllUnavailable,
        _ => ProcessError::Provider(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use buddy_core::provider::ProviderChain;
    use buddy_core::testutil::{MockProvider, MockResponse, SequencedProvider};

    // ── Test 1: new chat creates conversation with source "telegram" ────

    #[tokio::test]
    async fn new_chat_creates_telegram_conversation() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["Hello!".into()],
        };

        let result = process_message(&store, &provider, 12345, "Hi there").await;
        assert!(matches!(result, Ok(ProcessResult::Response(_))));

        // Verify conversation was created with source "telegram".
        let convs = store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].source, "telegram");

        // Verify the user message was stored.
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        assert!(conv.messages.len() >= 2); // user + assistant
        assert_eq!(conv.messages[0].role, Role::User);
        assert!(matches!(
            &conv.messages[0].content,
            MessageContent::Text { text } if text == "Hi there"
        ));
    }

    // ── Test 2: provider called with history, response returned ─────────

    #[tokio::test]
    async fn provider_called_with_history_and_response_returned() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["I can ".into(), "help!".into()],
        };

        let result = process_message(&store, &provider, 100, "Help me").await;
        let response = match result {
            Ok(ProcessResult::Response(text)) => text,
            other => panic!("expected Response, got {other:?}"),
        };
        assert_eq!(response, "I can help!");

        // Verify assistant message was persisted.
        let convs = store.list_conversations().unwrap();
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        let last = conv.messages.last().unwrap();
        assert_eq!(last.role, Role::Assistant);
        assert!(matches!(
            &last.content,
            MessageContent::Text { text } if text == "I can help!"
        ));
    }

    // ── Test 3: same chat_id reuses conversation ────────────────────────

    #[tokio::test]
    async fn same_chat_id_reuses_conversation() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["ok".into()],
        };

        process_message(&store, &provider, 555, "First message")
            .await
            .unwrap();
        process_message(&store, &provider, 555, "Second message")
            .await
            .unwrap();

        // Only one conversation should exist.
        let convs = store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);

        // Both user messages + both assistant replies should be in the same conversation.
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        assert_eq!(conv.messages.len(), 4); // user, assistant, user, assistant
    }

    // ── Test 4: tool call declined with informational message ───────────

    #[tokio::test]
    async fn tool_call_declined_with_message() {
        let store = Store::open_in_memory().unwrap();
        let provider = SequencedProvider::new(vec![MockResponse::ToolCalls(vec![(
            "call_1".into(),
            "get_weather".into(),
            r#"{"city":"NYC"}"#.into(),
        )])]);

        let result = process_message(&store, &provider, 200, "What's the weather?").await;
        let response = match result {
            Ok(ProcessResult::Response(text)) => text,
            other => panic!("expected Response, got {other:?}"),
        };

        assert!(response.contains("get_weather"));
        assert!(response.contains("not yet available over Telegram"));
    }

    // ── Test 5: provider error returns user-friendly message ────────────

    #[tokio::test]
    async fn provider_error_returns_friendly_message() {
        let store = Store::open_in_memory().unwrap();

        // Use a chain with a single failing provider to simulate "all unavailable".
        let chain = ProviderChain::new(vec![(
            buddy_core::testutil::ConfigurableMockProvider::FailNetwork(
                "connection refused".into(),
            ),
            "test-model".into(),
        )]);

        let result = process_message(&store, &chain, 300, "Hello").await;
        match result {
            Err(ProcessError::AllUnavailable) => {}
            other => panic!("expected AllUnavailable, got {other:?}"),
        }
    }

    impl std::fmt::Debug for ProcessResult {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Response(text) => write!(f, "Response({text:?})"),
                Self::Empty => write!(f, "Empty"),
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
}
