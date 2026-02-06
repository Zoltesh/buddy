use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;

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
    system_prompt: String,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        Self {
            client: Client::new(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            endpoint: config.endpoint.clone(),
            system_prompt: config.system_prompt.clone(),
        }
    }
}

// --- Request body construction ---

/// Convert our internal `Message` list into the OpenAI messages JSON format.
///
/// Consecutive `ToolCall` messages from the assistant are grouped into a single
/// assistant message with a `tool_calls` array. `ToolResult` messages become
/// `role: "tool"` messages with `tool_call_id`.
fn to_chat_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    let mut result: Vec<serde_json::Value> = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        let msg = &messages[i];
        match &msg.content {
            MessageContent::Text { text } => {
                let role = match msg.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => "system",
                };
                result.push(serde_json::json!({
                    "role": role,
                    "content": text,
                }));
                i += 1;
            }
            MessageContent::ToolCall {
                id,
                name,
                arguments,
            } => {
                // Collect consecutive assistant tool calls into one message.
                let mut tool_calls = vec![serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments,
                    }
                })];
                i += 1;
                while i < messages.len() {
                    if let MessageContent::ToolCall {
                        id,
                        name,
                        arguments,
                    } = &messages[i].content
                    {
                        tool_calls.push(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments,
                            }
                        }));
                        i += 1;
                    } else {
                        break;
                    }
                }
                result.push(serde_json::json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": tool_calls,
                }));
            }
            MessageContent::ToolResult { id, content } => {
                result.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": id,
                    "content": content,
                }));
                i += 1;
            }
        }
    }

    result
}

/// Build the full request body for the OpenAI chat completions endpoint.
fn build_request_body(
    messages: &[Message],
    model: &str,
    system_prompt: &str,
    tools: Option<&Vec<serde_json::Value>>,
) -> serde_json::Value {
    let mut chat_messages = Vec::new();
    if !system_prompt.is_empty() {
        chat_messages.push(serde_json::json!({
            "role": "system",
            "content": system_prompt,
        }));
    }
    chat_messages.extend(to_chat_messages(messages));

    let mut body = serde_json::json!({
        "model": model,
        "messages": chat_messages,
        "stream": true,
    });

    if let Some(tools) = tools {
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(tools);
        }
    }

    body
}

// --- Response types ---

#[derive(Deserialize)]
struct ChatChunk {
    choices: Vec<ChunkChoice>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChunkDelta {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallChunk>>,
}

#[derive(Deserialize, Clone, Debug)]
struct ToolCallChunk {
    index: usize,
    id: Option<String>,
    function: Option<ToolCallFunctionChunk>,
}

#[derive(Deserialize, Clone, Debug)]
struct ToolCallFunctionChunk {
    name: Option<String>,
    arguments: Option<String>,
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

/// What a single SSE line resolved to after parsing.
#[derive(Debug)]
enum SseChunk {
    /// Text content delta.
    TextDelta(String),
    /// Partial tool call data to accumulate.
    ToolCallDelta(Vec<ToolCallChunk>),
    /// The model finished with tool calls — drain the accumulator.
    FinishToolCalls,
    /// Nothing actionable.
    Empty,
}

/// Parse a single `data:` line from the SSE stream.
fn parse_sse_line(line: &str) -> Result<SseChunk, ProviderError> {
    let data = match line.strip_prefix("data: ") {
        Some(d) => d,
        None => return Ok(SseChunk::Empty),
    };

    if data == "[DONE]" {
        return Ok(SseChunk::Empty);
    }

    let chunk: ChatChunk = serde_json::from_str(data)
        .map_err(|e| ProviderError::MalformedResponse(format!("invalid JSON in SSE: {e}")))?;

    if let Some(choice) = chunk.choices.first() {
        // Check for tool call deltas first.
        if let Some(ref tc_deltas) = choice.delta.tool_calls {
            // If this chunk also carries the finish_reason we'll handle it
            // after accumulating.
            let is_finish = choice
                .finish_reason
                .as_deref()
                .is_some_and(|r| r == "tool_calls");

            if is_finish {
                // The deltas in this chunk still need accumulating, but we
                // signal FinishToolCalls afterward. Return both via the delta
                // path — the caller accumulates, then we return finish.
                // To keep parse_sse_line stateless, the streaming code will
                // handle this: accumulate deltas, then check finish_reason
                // separately. Let's just return the delta here; the streaming
                // loop peeks at finish_reason via a second field.
            }
            return Ok(SseChunk::ToolCallDelta(tc_deltas.clone()));
        }

        // Check finish_reason for tool_calls (no deltas in this chunk).
        if choice
            .finish_reason
            .as_deref()
            .is_some_and(|r| r == "tool_calls")
        {
            return Ok(SseChunk::FinishToolCalls);
        }

        // Text content.
        if let Some(ref text) = choice.delta.content {
            if !text.is_empty() {
                return Ok(SseChunk::TextDelta(text.clone()));
            }
        }
    }

    Ok(SseChunk::Empty)
}

/// Accumulator for tool call chunks that arrive across multiple SSE events.
struct ToolCallAccumulator {
    /// (id, name, arguments_buffer) indexed by the tool_call index.
    calls: Vec<(String, String, String)>,
}

impl ToolCallAccumulator {
    fn new() -> Self {
        Self { calls: Vec::new() }
    }

    fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }

    fn process(&mut self, chunks: &[ToolCallChunk]) {
        for tc in chunks {
            while self.calls.len() <= tc.index {
                self.calls.push((String::new(), String::new(), String::new()));
            }
            if let Some(ref id) = tc.id {
                self.calls[tc.index].0.clone_from(id);
            }
            if let Some(ref func) = tc.function {
                if let Some(ref name) = func.name {
                    self.calls[tc.index].1.clone_from(name);
                }
                if let Some(ref args) = func.arguments {
                    self.calls[tc.index].2.push_str(args);
                }
            }
        }
    }

    fn drain(&mut self) -> Vec<Token> {
        std::mem::take(&mut self.calls)
            .into_iter()
            .map(|(id, name, arguments)| Token::ToolCall {
                id,
                name,
                arguments,
            })
            .collect()
    }
}

/// Map an HTTP error status code and body to a `ProviderError`.
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
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        let url = format!(
            "{}/chat/completions",
            self.endpoint.trim_end_matches('/')
        );
        let body = build_request_body(&messages, &self.model, &self.system_prompt, tools.as_ref());

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
            let mut tool_acc = ToolCallAccumulator::new();

            while let Some(chunk) = byte_stream.next().await {
                let bytes = chunk.map_err(|e| ProviderError::Network(e.to_string()))?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim_end_matches('\r').to_string();
                    buffer = buffer[pos + 1..].to_string();

                    match parse_sse_line(&line)? {
                        SseChunk::TextDelta(text) => yield Token::Text { text },
                        SseChunk::ToolCallDelta(chunks) => {
                            tool_acc.process(&chunks);
                        }
                        SseChunk::FinishToolCalls => {
                            for token in tool_acc.drain() {
                                yield token;
                            }
                        }
                        SseChunk::Empty => {}
                    }
                }
            }

            // Flush remaining buffer.
            let remaining = buffer.trim();
            if !remaining.is_empty() {
                match parse_sse_line(remaining)? {
                    SseChunk::TextDelta(text) => yield Token::Text { text },
                    SseChunk::FinishToolCalls => {
                        for token in tool_acc.drain() {
                            yield token;
                        }
                    }
                    SseChunk::ToolCallDelta(chunks) => {
                        // Last line was a delta with no explicit finish — drain anyway.
                        tool_acc.process(&chunks);
                        for token in tool_acc.drain() {
                            yield token;
                        }
                    }
                    SseChunk::Empty => {}
                }
            }

            // If tool calls were accumulated but never flushed (no finish_reason
            // line), drain them now.
            if !tool_acc.is_empty() {
                for token in tool_acc.drain() {
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
        let body = build_request_body(&messages, "gpt-4", "You are a helpful assistant.", None);

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["stream"], true);

        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are a helpful assistant.");
        assert_eq!(msgs[1]["role"], "system");
        assert_eq!(msgs[1]["content"], "You are helpful.");
        assert_eq!(msgs[2]["role"], "user");
        assert_eq!(msgs[2]["content"], "Hi");

        // No internal fields leak.
        assert!(msgs[1].get("timestamp").is_none());
        assert!(msgs[1].get("type").is_none());
    }

    #[test]
    fn request_body_includes_tools_when_provided() {
        let messages = make_messages();
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": { "type": "object" }
            }
        })];
        let body =
            build_request_body(&messages, "gpt-4", "", Some(&tools));

        assert!(body["tools"].is_array());
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn request_body_omits_tools_when_none() {
        let messages = make_messages();
        let body = build_request_body(&messages, "gpt-4", "", None);
        assert!(body.get("tools").is_none());
    }

    #[test]
    fn empty_system_prompt_is_not_prepended() {
        let messages = make_messages();
        let body = build_request_body(&messages, "gpt-4", "", None);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
    }

    #[test]
    fn tool_call_messages_are_grouped() {
        let now = Utc::now();
        let messages = vec![
            Message {
                role: Role::User,
                content: MessageContent::Text {
                    text: "Read two files".into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: "call_1".into(),
                    name: "read_file".into(),
                    arguments: r#"{"path":"a.txt"}"#.into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: "call_2".into(),
                    name: "read_file".into(),
                    arguments: r#"{"path":"b.txt"}"#.into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: "call_1".into(),
                    content: "contents of a".into(),
                },
                timestamp: now,
            },
            Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: "call_2".into(),
                    content: "contents of b".into(),
                },
                timestamp: now,
            },
        ];

        let chat = to_chat_messages(&messages);
        // user text, assistant with 2 tool_calls, 2 tool results
        assert_eq!(chat.len(), 4);

        assert_eq!(chat[0]["role"], "user");
        assert_eq!(chat[1]["role"], "assistant");
        let tcs = chat[1]["tool_calls"].as_array().unwrap();
        assert_eq!(tcs.len(), 2);
        assert_eq!(tcs[0]["id"], "call_1");
        assert_eq!(tcs[1]["id"], "call_2");

        assert_eq!(chat[2]["role"], "tool");
        assert_eq!(chat[2]["tool_call_id"], "call_1");
        assert_eq!(chat[3]["role"], "tool");
        assert_eq!(chat[3]["tool_call_id"], "call_2");
    }

    #[test]
    fn parse_sse_text_tokens() {
        let lines = [
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"gpt-4","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ];

        let mut texts = Vec::new();
        for line in &lines {
            if let SseChunk::TextDelta(t) = parse_sse_line(line).unwrap() {
                texts.push(t);
            }
        }
        assert_eq!(texts, vec!["Hello", " world"]);
    }

    #[test]
    fn parse_sse_tool_call_chunks() {
        let lines = [
            r#"data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_abc","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}"#,
            r#"data: {"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pa"}}]},"finish_reason":null}]}"#,
            r#"data: {"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"a.txt\"}"}}]},"finish_reason":null}]}"#,
            r#"data: {"id":"1","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#,
            "data: [DONE]",
        ];

        let mut acc = ToolCallAccumulator::new();
        let mut finished = false;

        for line in &lines {
            match parse_sse_line(line).unwrap() {
                SseChunk::ToolCallDelta(chunks) => acc.process(&chunks),
                SseChunk::FinishToolCalls => finished = true,
                _ => {}
            }
        }

        assert!(finished);
        let tokens = acc.drain();
        assert_eq!(tokens.len(), 1);
        assert_eq!(
            tokens[0],
            Token::ToolCall {
                id: "call_abc".into(),
                name: "read_file".into(),
                arguments: r#"{"path":"a.txt"}"#.into(),
            }
        );
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
    fn non_data_sse_lines_are_ignored() {
        assert!(matches!(parse_sse_line("").unwrap(), SseChunk::Empty));
        assert!(matches!(
            parse_sse_line(": keep-alive").unwrap(),
            SseChunk::Empty
        ));
        assert!(matches!(
            parse_sse_line("event: ping").unwrap(),
            SseChunk::Empty
        ));
    }

    #[tokio::test]
    #[ignore] // Requires a live API key: OPENAI_API_KEY=... cargo test -- --ignored
    async fn integration_live_streaming() {
        let config = ProviderConfig {
            api_key: std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set"),
            model: "gpt-4".into(),
            endpoint: "https://api.openai.com/v1".into(),
            system_prompt: "You are a helpful assistant.".into(),
        };
        let provider = OpenAiProvider::new(&config);

        let messages = vec![Message {
            role: Role::User,
            content: MessageContent::Text {
                text: "Say hello in one word.".into(),
            },
            timestamp: Utc::now(),
        }];

        let mut stream = provider.complete(messages, None).await.unwrap();
        let mut full_text = String::new();
        while let Some(result) = stream.next().await {
            match result.unwrap() {
                Token::Text { text } => full_text.push_str(&text),
                _ => {}
            }
        }
        assert!(!full_text.is_empty(), "expected non-empty response");
    }
}
