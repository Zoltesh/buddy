//! Conversation processing for WhatsApp.
//!
//! Receives a user message, resolves the conversation, calls the provider
//! chain, and returns the response text. Tool calls are gracefully declined.

use buddy_core::provider::{Provider, ProviderError, Token};
use buddy_core::skill::SkillRegistry;
use buddy_core::store::Store;
use buddy_core::types::{Message, MessageContent, Role};
use chrono::Utc;
use futures_util::StreamExt;

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
        "Sorry, I couldn't process that. Please try again."
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

/// Process a WhatsApp text message: resolve conversation, run provider,
/// and return the response text.
///
/// Tool calls are not executed; instead an informational message is returned.
pub async fn process_message<P: Provider>(
    store: &Store,
    provider: &P,
    registry: &SkillRegistry,
    phone: &str,
    user_text: &str,
) -> Result<String, ProcessError> {
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
        Ok(None) => vec![user_msg],
        Err(e) => {
            log::error!("Failed to load conversation: {e}");
            return Err(ProcessError::Store(e));
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

    let token_stream = provider
        .complete(messages, tools)
        .await
        .map_err(|e| {
            log::error!("Provider error: {e}");
            classify_provider_error(e)
        })?;

    let mut full_text = String::new();
    let mut tool_names: Vec<String> = Vec::new();

    tokio::pin!(token_stream);
    while let Some(result) = token_stream.next().await {
        match result {
            Ok(Token::Text { text }) => full_text.push_str(&text),
            Ok(Token::ToolCall { name, .. }) => tool_names.push(name),
            Ok(Token::Warning { message }) => log::warn!("Provider warning: {message}"),
            Err(e) => {
                log::error!("Stream error: {e}");
                return Err(classify_provider_error(e));
            }
        }
    }

    let response_text = if !tool_names.is_empty() {
        tool_names
            .iter()
            .map(|name| {
                format!(
                    "I wanted to use a tool ({name}), but tool execution is not yet available over WhatsApp."
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else if full_text.is_empty() {
        return Ok(String::new());
    } else {
        full_text
    };

    let assistant_msg = Message {
        role: Role::Assistant,
        content: MessageContent::Text {
            text: response_text.clone(),
        },
        timestamp: Utc::now(),
    };
    if let Err(e) = store.append_message(&conversation_id, &assistant_msg) {
        log::error!("Failed to append assistant message: {e}");
    }

    Ok(response_text)
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
    use super::*;
    use buddy_core::testutil::{MockProvider, MockResponse, SequencedProvider};

    #[tokio::test]
    async fn new_sender_creates_whatsapp_conversation() {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["Hello!".into()],
        };
        let registry = SkillRegistry::new();

        let result = process_message(&store, &provider, &registry, "15559876543", "Hi there").await;
        assert!(result.is_ok());

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

        let result = process_message(&store, &provider, &registry, "15551234567", "Help me")
            .await
            .unwrap();
        assert_eq!(result, "I can help!");

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

        process_message(&store, &provider, &registry, "15559876543", "First")
            .await
            .unwrap();
        process_message(&store, &provider, &registry, "15559876543", "Second")
            .await
            .unwrap();

        let convs = store.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
        assert_eq!(conv.messages.len(), 4); // 2 user + 2 assistant
    }

    #[tokio::test]
    async fn tool_call_declined_with_informational_message() {
        let store = Store::open_in_memory().unwrap();
        let provider = SequencedProvider::new(vec![MockResponse::ToolCalls(vec![(
            "c1".into(),
            "get_weather".into(),
            r#"{"city":"NYC"}"#.into(),
        )])]);
        let registry = SkillRegistry::new();

        let result =
            process_message(&store, &provider, &registry, "15551234567", "What's the weather?")
                .await
                .unwrap();
        assert!(result.contains("get_weather"));
        assert!(result.contains("not yet available over WhatsApp"));
    }

    #[tokio::test]
    async fn provider_error_returns_user_friendly_message() {
        let store = Store::open_in_memory().unwrap();
        let provider = buddy_core::testutil::ConfigurableMockProvider::FailNetwork(
            "connection refused".into(),
        );
        let registry = SkillRegistry::new();

        let result =
            process_message(&store, &provider, &registry, "15551234567", "Hello").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.user_message(),
            "Sorry, I couldn't process that. Please try again."
        );
    }
}
