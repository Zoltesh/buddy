use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        id: String,
        content: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn round_trip_multi_turn_conversation() {
        let now = Utc::now();
        let conversation = vec![
            Message {
                role: Role::System,
                content: MessageContent::Text {
                    text: "You are a helpful assistant.".into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::User,
                content: MessageContent::Text {
                    text: "Hello!".into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::Text {
                    text: "Hi there! How can I help?".into(),
                },
                timestamp: now,
            },
        ];

        let json = serde_json::to_string(&conversation).expect("serialize");
        let deserialized: Vec<Message> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(conversation, deserialized);
    }

    #[test]
    fn round_trip_tool_call() {
        let msg = Message {
            role: Role::Assistant,
            content: MessageContent::ToolCall {
                id: "call_123".into(),
                name: "get_weather".into(),
                arguments: r#"{"location":"NYC"}"#.into(),
            },
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&msg).expect("serialize");
        let deserialized: Message = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn round_trip_tool_result() {
        let msg = Message {
            role: Role::User,
            content: MessageContent::ToolResult {
                id: "call_123".into(),
                content: "72°F and sunny".into(),
            },
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&msg).expect("serialize");
        let deserialized: Message = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(msg, deserialized);
    }
}
