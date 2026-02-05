use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::ProviderConfig;
use crate::provider::{Provider, ProviderError, Token, TokenStream};
use crate::types::{Message, MessageContent, Role};

/// OpenAI-compatible provider (works with OpenAI, Azure OpenAI, and any
/// endpoint that speaks the same chat-completions protocol).
pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        Self {
            client: Client::new(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            endpoint: config.endpoint.clone(),
        }
    }
}

// --- Request types ---

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage>,
    stream: bool,
}

#[derive(Serialize, Debug, PartialEq)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

fn to_chat_messages(messages: &[Message]) -> Vec<ChatMessage> {
    messages
        .iter()
        .filter_map(|msg| {
            let content = match &msg.content {
                MessageContent::Text { text } => text.clone(),
                _ => return None,
            };
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };
            Some(ChatMessage { role, content })
        })
        .collect()
}

fn build_request_body<'a>(messages: &[Message], model: &'a str) -> ChatRequest<'a> {
    ChatRequest {
        model,
        messages: to_chat_messages(messages),
        stream: true,
    }
}

// --- Response types ---

#[derive(Deserialize)]
struct ChatChunk {
    choices: Vec<ChunkChoice>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
}

#[derive(Deserialize)]
struct ChunkDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
    message: String,
}

// --- SSE parsing ---

/// Parse a single line from an SSE stream into an optional Token.
fn parse_sse_line(line: &str) -> Result<Option<Token>, ProviderError> {
    let data = match line.strip_prefix("data: ") {
        Some(d) => d,
        None => return Ok(None),
    };

    if data == "[DONE]" {
        return Ok(None);
    }

    let chunk: ChatChunk = serde_json::from_str(data)
        .map_err(|e| ProviderError::MalformedResponse(format!("invalid JSON in SSE: {e}")))?;

    if let Some(choice) = chunk.choices.first() {
        if let Some(ref text) = choice.delta.content {
            if !text.is_empty() {
                return Ok(Some(Token { text: text.clone() }));
            }
        }
    }

    Ok(None)
}

/// Map an HTTP error status code and body to a ProviderError.
fn map_error_status(status: u16, body: &str) -> ProviderError {
    let message = serde_json::from_str::<ErrorResponse>(body)
        .map(|r| r.error.message)
        .unwrap_or_else(|_| body.to_string());

    match status {
        401 => ProviderError::Auth(message),
        429 => ProviderError::RateLimit(message),
        _ => ProviderError::Other(format!("HTTP {status}: {message}")),
    }
}

// --- Provider implementation ---

impl Provider for OpenAiProvider {
    async fn complete(&self, messages: Vec<Message>) -> Result<TokenStream, ProviderError> {
        let url = format!(
            "{}/chat/completions",
            self.endpoint.trim_end_matches('/')
        );
        let body = build_request_body(&messages, &self.model);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_error_status(status.as_u16(), &body_text));
        }

        let stream = async_stream::try_stream! {
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk) = byte_stream.next().await {
                let bytes = chunk.map_err(|e| ProviderError::Network(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if let Some(token) = parse_sse_line(&line)? {
                        yield token;
                    }
                }
            }

            let remaining = buffer.trim();
            if !remaining.is_empty() {
                if let Some(token) = parse_sse_line(remaining)? {
                    yield token;
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_messages() -> Vec<Message> {
        let now = Utc::now();
        vec![
            Message {
                role: Role::System,
                content: MessageContent::Text {
                    text: "You are helpful.".into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::User,
                content: MessageContent::Text {
                    text: "Hi".into(),
                },
                timestamp: now,
            },
        ]
    }

    #[test]
    fn request_body_matches_openai_spec() {
        let messages = make_messages();
        let body = build_request_body(&messages, "gpt-4");
        let json = serde_json::to_value(&body).unwrap();

        assert_eq!(json["model"], "gpt-4");
        assert_eq!(json["stream"], true);

        let msgs = json["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "Hi");

        // No internal fields leak into the request
        assert!(msgs[0].get("timestamp").is_none());
        assert!(msgs[0].get("type").is_none());
    }

    #[test]
    fn parse_sse_stream_into_tokens() {
        let lines = [
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ];

        let tokens: Vec<Token> = lines
            .iter()
            .filter_map(|line| parse_sse_line(line).unwrap())
            .collect();

        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text, "Hello");
        assert_eq!(tokens[1].text, " world");
    }

    #[test]
    fn parse_malformed_sse_returns_error() {
        let line = "data: {not valid json}";
        let result = parse_sse_line(line);
        assert!(result.is_err());
        match result.unwrap_err() {
            ProviderError::MalformedResponse(msg) => {
                assert!(msg.contains("invalid JSON"), "error: {msg}");
            }
            other => panic!("expected MalformedResponse, got: {other:?}"),
        }
    }

    #[test]
    fn error_status_401_maps_to_auth() {
        let body = r#"{"error":{"message":"Invalid API key","type":"invalid_request_error","code":"invalid_api_key"}}"#;
        match map_error_status(401, body) {
            ProviderError::Auth(msg) => assert_eq!(msg, "Invalid API key"),
            other => panic!("expected Auth, got: {other:?}"),
        }
    }

    #[test]
    fn error_status_429_maps_to_rate_limit() {
        let body = r#"{"error":{"message":"Rate limit exceeded","type":"rate_limit_error","code":"rate_limit"}}"#;
        match map_error_status(429, body) {
            ProviderError::RateLimit(msg) => assert_eq!(msg, "Rate limit exceeded"),
            other => panic!("expected RateLimit, got: {other:?}"),
        }
    }

    #[test]
    fn error_status_500_maps_to_other() {
        let body = "Internal Server Error";
        match map_error_status(500, body) {
            ProviderError::Other(msg) => assert!(msg.contains("500"), "msg: {msg}"),
            other => panic!("expected Other, got: {other:?}"),
        }
    }

    #[test]
    fn tool_messages_are_skipped_in_conversion() {
        let messages = vec![
            Message {
                role: Role::User,
                content: MessageContent::Text {
                    text: "Hello".into(),
                },
                timestamp: Utc::now(),
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: "call_1".into(),
                    name: "get_weather".into(),
                    arguments: "{}".into(),
                },
                timestamp: Utc::now(),
            },
            Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: "call_1".into(),
                    content: "72F".into(),
                },
                timestamp: Utc::now(),
            },
        ];

        let chat_msgs = to_chat_messages(&messages);
        assert_eq!(chat_msgs.len(), 1);
        assert_eq!(chat_msgs[0].content, "Hello");
    }

    #[test]
    fn non_data_sse_lines_are_ignored() {
        assert!(parse_sse_line("").unwrap().is_none());
        assert!(parse_sse_line(": keep-alive").unwrap().is_none());
        assert!(parse_sse_line("event: ping").unwrap().is_none());
    }

    #[tokio::test]
    #[ignore] // Requires a live API key: OPENAI_API_KEY=... cargo test -- --ignored
    async fn integration_live_streaming() {
        let config = ProviderConfig {
            api_key: std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set"),
            model: "gpt-4".into(),
            endpoint: "https://api.openai.com/v1".into(),
        };
        let provider = OpenAiProvider::new(&config);

        let messages = vec![Message {
            role: Role::User,
            content: MessageContent::Text {
                text: "Say hello in one word.".into(),
            },
            timestamp: Utc::now(),
        }];

        let mut stream = provider.complete(messages).await.unwrap();
        let mut full_text = String::new();
        while let Some(result) = stream.next().await {
            let token = result.unwrap();
            full_text.push_str(&token.text);
        }
        assert!(!full_text.is_empty(), "expected non-empty response");
    }
}
