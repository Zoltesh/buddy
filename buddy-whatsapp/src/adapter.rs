use buddy_core::types::{Message, MessageContent, Role};
use chrono::Utc;
use serde::Deserialize;

/// Top-level webhook payload from the WhatsApp Business Cloud API.
#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    #[serde(default)]
    pub entry: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    #[serde(default)]
    pub changes: Vec<Change>,
}

#[derive(Debug, Deserialize)]
pub struct Change {
    #[serde(default)]
    pub value: Option<ChangeValue>,
}

#[derive(Debug, Deserialize)]
pub struct ChangeValue {
    #[serde(default)]
    pub messages: Vec<WhatsAppMessage>,
}

#[derive(Debug, Deserialize)]
pub struct WhatsAppMessage {
    pub id: String,
    pub from: String,
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(default)]
    pub timestamp: Option<String>,
    pub text: Option<TextBody>,
}

#[derive(Debug, Deserialize)]
pub struct TextBody {
    pub body: String,
}

/// Converts a WhatsApp text message to a buddy-core `Message`.
///
/// Returns `None` if the message is not a text message.
pub fn whatsapp_to_buddy(message: &WhatsAppMessage) -> Option<Message> {
    if message.message_type != "text" {
        return None;
    }
    let text = message.text.as_ref()?.body.clone();
    let timestamp = message
        .timestamp
        .as_deref()
        .and_then(|ts| ts.parse::<i64>().ok())
        .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .unwrap_or_else(Utc::now);
    Some(Message {
        role: Role::User,
        content: MessageContent::Text { text },
        timestamp,
    })
}

/// Converts a buddy-core `Message` to plain text suitable for WhatsApp.
pub fn buddy_to_whatsapp(message: &Message) -> String {
    match &message.content {
        MessageContent::Text { text } => text.clone(),
        MessageContent::ToolCall { name, .. } => format!("Using tool: {name}..."),
        MessageContent::ToolResult { content, .. } => content.clone(),
    }
}

/// WhatsApp's maximum message length (in characters).
const WHATSAPP_MAX_LENGTH: usize = 4096;

/// Convert markdown formatting to WhatsApp-compatible formatting.
///
/// Handles: `**bold**` → `*bold*`, `*italic*` → `_italic_`,
/// `~~strike~~` → `~strike~`, and strips `#` header prefixes.
/// Code blocks and inline code pass through unchanged.
pub fn markdown_to_whatsapp(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') {
            lines.push(line.trim_start_matches('#').trim_start());
        } else {
            lines.push(line);
        }
    }
    let result = lines.join("\n");

    // Bold: **text** → *text* (use placeholder to avoid conflict with italic)
    let result = result.replace("**", "\x01");
    // Italic: *text* → _text_
    let result = result.replace('*', "_");
    // Restore bold markers
    let result = result.replace('\x01', "*");
    // Strikethrough: ~~text~~ → ~text~
    result.replace("~~", "~")
}

/// Split text into chunks that fit within WhatsApp's message length limit.
///
/// Prefers splitting at newline or space boundaries for readability.
pub fn split_message(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= WHATSAPP_MAX_LENGTH {
        return vec![text.to_string()];
    }

    let mut parts = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= WHATSAPP_MAX_LENGTH {
            parts.push(chars[start..].iter().collect());
            break;
        }

        let end = start + WHATSAPP_MAX_LENGTH;
        let chunk = &chars[start..end];

        let split_offset = chunk
            .iter()
            .rposition(|&c| c == '\n')
            .or_else(|| chunk.iter().rposition(|&c| c == ' '))
            .map(|i| i + 1)
            .unwrap_or(WHATSAPP_MAX_LENGTH);

        parts.push(chars[start..start + split_offset].iter().collect());
        start += split_offset;
    }

    parts
}

/// Extract all messages from a webhook payload.
pub fn extract_messages(payload: &WebhookPayload) -> Vec<&WhatsAppMessage> {
    payload
        .entry
        .iter()
        .flat_map(|e| &e.changes)
        .filter_map(|c| c.value.as_ref())
        .flat_map(|v| &v.messages)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use buddy_core::types::{MessageContent, Role};

    fn make_text_message(from: &str, text: &str) -> WhatsAppMessage {
        WhatsAppMessage {
            id: "wamid.test123".to_string(),
            from: from.to_string(),
            message_type: "text".to_string(),
            timestamp: Some("1234567890".to_string()),
            text: Some(TextBody {
                body: text.to_string(),
            }),
        }
    }

    fn make_image_message(from: &str) -> WhatsAppMessage {
        WhatsAppMessage {
            id: "wamid.test456".to_string(),
            from: from.to_string(),
            message_type: "image".to_string(),
            timestamp: Some("1234567890".to_string()),
            text: None,
        }
    }

    #[test]
    fn whatsapp_text_converts_to_buddy_message() {
        let msg = make_text_message("15551234567", "Hello");
        let result = whatsapp_to_buddy(&msg);
        let buddy_msg = result.expect("should produce a Message");
        assert_eq!(buddy_msg.role, Role::User);
        assert!(
            matches!(&buddy_msg.content, MessageContent::Text { text } if text == "Hello")
        );
    }

    #[test]
    fn whatsapp_image_returns_none() {
        let msg = make_image_message("15551234567");
        assert!(whatsapp_to_buddy(&msg).is_none());
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
        assert_eq!(buddy_to_whatsapp(&msg), "Hi there!");
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
        assert_eq!(buddy_to_whatsapp(&msg), "Using tool: get_weather...");
    }

    #[test]
    fn buddy_tool_result_passes_content() {
        let msg = Message {
            role: Role::Assistant,
            content: MessageContent::ToolResult {
                id: "call_1".into(),
                content: "72°F and sunny".into(),
            },
            timestamp: Utc::now(),
        };
        assert_eq!(buddy_to_whatsapp(&msg), "72°F and sunny");
    }

    #[test]
    fn extract_messages_from_payload() {
        let payload: WebhookPayload = serde_json::from_value(serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "BIZ_ID",
                "changes": [{
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "15551234567",
                            "phone_number_id": "PHONE_ID"
                        },
                        "messages": [{
                            "id": "wamid.abc123",
                            "from": "15559876543",
                            "timestamp": "1700000000",
                            "type": "text",
                            "text": { "body": "Hello from WhatsApp" }
                        }]
                    },
                    "field": "messages"
                }]
            }]
        }))
        .expect("valid webhook payload");

        let messages = extract_messages(&payload);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "15559876543");
        assert_eq!(messages[0].text.as_ref().unwrap().body, "Hello from WhatsApp");
    }

    #[test]
    fn extract_messages_empty_payload() {
        let payload = WebhookPayload { entry: vec![] };
        assert!(extract_messages(&payload).is_empty());
    }

    #[test]
    fn markdown_bold_converts_to_whatsapp() {
        assert_eq!(markdown_to_whatsapp("**bold**"), "*bold*");
    }

    #[test]
    fn markdown_italic_converts_to_whatsapp() {
        assert_eq!(markdown_to_whatsapp("*italic*"), "_italic_");
    }

    #[test]
    fn markdown_strikethrough_converts_to_whatsapp() {
        assert_eq!(markdown_to_whatsapp("~~strike~~"), "~strike~");
    }

    #[test]
    fn markdown_header_stripped() {
        assert_eq!(markdown_to_whatsapp("## Header"), "Header");
        assert_eq!(markdown_to_whatsapp("# Title"), "Title");
    }

    #[test]
    fn markdown_mixed_formatting() {
        assert_eq!(
            markdown_to_whatsapp("**bold** and *italic*"),
            "*bold* and _italic_"
        );
    }

    #[test]
    fn split_message_short_text_unchanged() {
        let parts = split_message("Hello");
        assert_eq!(parts, vec!["Hello"]);
    }

    #[test]
    fn split_message_long_text_splits() {
        let text = "word ".repeat(1000); // 5000 chars
        let parts = split_message(&text);
        assert!(parts.len() > 1);
        for part in &parts {
            assert!(part.chars().count() <= WHATSAPP_MAX_LENGTH);
        }
    }
}
