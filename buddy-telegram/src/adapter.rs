use buddy_core::types::{Message, MessageContent, Role};
use chrono::Utc;

/// Telegram's maximum message length (in characters after entity parsing).
const TELEGRAM_MAX_LENGTH: usize = 4096;

/// Converts a Telegram message to a buddy-core `Message`.
///
/// Returns `None` if the Telegram message contains no text (e.g. photos, stickers).
pub fn telegram_to_buddy(message: &teloxide::types::Message) -> Option<Message> {
    let text = message.text()?;
    Some(Message {
        role: Role::User,
        content: MessageContent::Text {
            text: text.to_string(),
        },
        timestamp: Utc::now(),
    })
}

/// Converts a buddy-core `Message` to a plain text string suitable for Telegram.
///
/// Tool calls are formatted as "Using tool: {name}..." and tool results
/// pass through their content directly.
pub fn buddy_to_telegram(message: &Message) -> String {
    match &message.content {
        MessageContent::Text { text } => text.clone(),
        MessageContent::ToolCall { name, .. } => format!("Using tool: {name}..."),
        MessageContent::ToolResult { content, .. } => content.clone(),
    }
}

/// Escape special characters for Telegram's MarkdownV2 parse mode.
pub fn escape_markdown_v2(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        if matches!(
            c,
            '_' | '*'
                | '['
                | ']'
                | '('
                | ')'
                | '~'
                | '`'
                | '>'
                | '#'
                | '+'
                | '-'
                | '='
                | '|'
                | '{'
                | '}'
                | '.'
                | '!'
                | '\\'
        ) {
            result.push('\\');
        }
        result.push(c);
    }
    result
}

/// Split text into chunks that fit within Telegram's message length limit.
///
/// Prefers splitting at newline or space boundaries for readability.
pub fn split_message(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= TELEGRAM_MAX_LENGTH {
        return vec![text.to_string()];
    }

    let mut parts = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= TELEGRAM_MAX_LENGTH {
            parts.push(chars[start..].iter().collect());
            break;
        }

        let end = start + TELEGRAM_MAX_LENGTH;
        let chunk = &chars[start..end];

        // Prefer splitting at a newline, then a space.
        let split_offset = chunk
            .iter()
            .rposition(|&c| c == '\n')
            .or_else(|| chunk.iter().rposition(|&c| c == ' '))
            .map(|i| i + 1)
            .unwrap_or(TELEGRAM_MAX_LENGTH);

        parts.push(chars[start..start + split_offset].iter().collect());
        start += split_offset;
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;
    use buddy_core::types::{MessageContent, Role};

    fn make_telegram_text_message(text: &str) -> teloxide::types::Message {
        serde_json::from_value(serde_json::json!({
            "message_id": 1,
            "date": 1_234_567_890,
            "chat": {
                "id": 12345,
                "type": "private"
            },
            "from": {
                "id": 67890,
                "is_bot": false,
                "first_name": "Test"
            },
            "text": text
        }))
        .expect("valid telegram text message JSON")
    }

    fn make_telegram_photo_message() -> teloxide::types::Message {
        serde_json::from_value(serde_json::json!({
            "message_id": 2,
            "date": 1_234_567_890,
            "chat": {
                "id": 12345,
                "type": "private"
            },
            "photo": [{
                "file_id": "abc",
                "file_unique_id": "def",
                "width": 100,
                "height": 100
            }]
        }))
        .expect("valid telegram photo message JSON")
    }

    #[test]
    fn telegram_text_converts_to_buddy_message() {
        let tg_msg = make_telegram_text_message("Hello");
        let result = telegram_to_buddy(&tg_msg);
        let buddy_msg = result.expect("should produce a Message");
        assert_eq!(buddy_msg.role, Role::User);
        assert!(
            matches!(&buddy_msg.content, MessageContent::Text { text } if text == "Hello")
        );
    }

    #[test]
    fn telegram_photo_returns_none() {
        let tg_msg = make_telegram_photo_message();
        assert!(telegram_to_buddy(&tg_msg).is_none());
    }

    #[test]
    fn buddy_text_converts_to_string() {
        let msg = Message {
            role: Role::Assistant,
            content: MessageContent::Text {
                text: "Hi there!".into(),
            },
            timestamp: Utc::now(),
        };
        assert_eq!(buddy_to_telegram(&msg), "Hi there!");
    }

    #[test]
    fn buddy_tool_call_converts_to_formatted_string() {
        let msg = Message {
            role: Role::Assistant,
            content: MessageContent::ToolCall {
                id: "call_1".into(),
                name: "get_weather".into(),
                arguments: r#"{"city":"NYC"}"#.into(),
            },
            timestamp: Utc::now(),
        };
        assert_eq!(buddy_to_telegram(&msg), "Using tool: get_weather...");
    }
}
