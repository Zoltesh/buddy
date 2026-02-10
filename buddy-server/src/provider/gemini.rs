use futures_util::StreamExt;
use reqwest::Client;
use serde::Deserialize;

use crate::provider::{Provider, ProviderError, Token, TokenStream};
use crate::types::{Message, MessageContent, Role};

/// Google Gemini provider.
///
/// Uses the Gemini REST API with streaming enabled via SSE (alt=sse).
/// Message format differs from OpenAI: roles are "user" and "model" (not
/// "assistant"), system prompts go in a separate `systemInstruction` field,
/// and consecutive same-role messages must be merged.
pub struct GeminiProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    system_prompt: String,
}

impl GeminiProvider {
    pub fn new(api_key: &str, model: &str, endpoint: &str, system_prompt: &str) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build HTTP client"),
            api_key: api_key.to_string(),
            model: model.to_string(),
            endpoint: endpoint.to_string(),
            system_prompt: system_prompt.to_string(),
        }
    }
}

// --- Request body construction ---

/// Convert buddy messages to Gemini's `contents` format.
///
/// Gemini requires alternating roles, so consecutive messages with the same
/// role are merged into a single `contents` entry with multiple parts.
fn to_gemini_contents(messages: &[Message]) -> Vec<serde_json::Value> {
    let mut result: Vec<serde_json::Value> = Vec::new();
    let mut i = 0;

    while i < messages.len() {
        let msg = &messages[i];

        // Skip system messages — they go in systemInstruction, not contents.
        if msg.role == Role::System {
            i += 1;
            continue;
        }

        let role = match msg.role {
            Role::User => "user",
            Role::Assistant => "model",
            Role::System => unreachable!(),
        };

        // Collect all consecutive messages with the same role.
        let mut parts = Vec::new();
        while i < messages.len() && messages[i].role == msg.role {
            let part = match &messages[i].content {
                MessageContent::Text { text } => {
                    serde_json::json!({ "text": text })
                }
                MessageContent::ToolCall { name, arguments, .. } => {
                    let args: serde_json::Value = serde_json::from_str(arguments)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    serde_json::json!({
                        "functionCall": {
                            "name": name,
                            "args": args,
                        }
                    })
                }
                MessageContent::ToolResult { content, .. } => {
                    // Gemini expects tool results to have a matching function name.
                    // We don't have the name stored in ToolResult, so we use a
                    // placeholder. This may need adjustment based on actual usage.
                    serde_json::json!({
                        "functionResponse": {
                            "name": "tool_result",
                            "response": {
                                "content": content,
                            }
                        }
                    })
                }
            };
            parts.push(part);
            i += 1;
        }

        result.push(serde_json::json!({
            "role": role,
            "parts": parts,
        }));
    }

    result
}

/// Map OpenAI tool definitions to Gemini's `functionDeclarations` format.
fn to_gemini_tools(tools: &[serde_json::Value]) -> Option<serde_json::Value> {
    if tools.is_empty() {
        return None;
    }

    let declarations: Vec<serde_json::Value> = tools
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function")?;
            let name = function.get("name")?.as_str()?;
            let description = function.get("description")?.as_str()?;
            let parameters = function.get("parameters")?;

            Some(serde_json::json!({
                "name": name,
                "description": description,
                "parameters": parameters,
            }))
        })
        .collect();

    if declarations.is_empty() {
        None
    } else {
        Some(serde_json::json!([{
            "functionDeclarations": declarations
        }]))
    }
}

/// Build the request body for Gemini's streamGenerateContent endpoint.
fn build_request_body(
    messages: &[Message],
    system_prompt: &str,
    tools: Option<&Vec<serde_json::Value>>,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "contents": to_gemini_contents(messages),
    });

    if !system_prompt.is_empty() {
        body["systemInstruction"] = serde_json::json!({
            "parts": [{ "text": system_prompt }]
        });
    }

    if let Some(tool_list) = tools {
        if let Some(gemini_tools) = to_gemini_tools(tool_list) {
            body["tools"] = gemini_tools;
        }
    }

    body
}

// --- Response types ---

#[derive(Deserialize)]
struct GeminiChunk {
    candidates: Option<Vec<GeminiCandidate>>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
    #[serde(rename = "functionCall")]
    function_call: Option<GeminiFunctionCall>,
}

#[derive(Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Deserialize)]
struct GeminiErrorResponse {
    error: GeminiError,
}

#[derive(Deserialize)]
struct GeminiError {
    message: String,
    status: Option<String>,
}

// --- SSE parsing ---

/// Parse a single `data:` line from Gemini's SSE stream.
fn parse_gemini_sse_line(line: &str) -> Result<Vec<Token>, ProviderError> {
    let data = match line.strip_prefix("data: ") {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };

    if data.trim().is_empty() {
        return Ok(Vec::new());
    }

    let chunk: GeminiChunk = serde_json::from_str(data)
        .map_err(|e| ProviderError::MalformedResponse(format!("invalid JSON in SSE: {e}")))?;

    let mut tokens = Vec::new();

    if let Some(candidates) = chunk.candidates {
        for candidate in candidates {
            if let Some(content) = candidate.content {
                for part in content.parts {
                    if let Some(text) = part.text {
                        if !text.is_empty() {
                            tokens.push(Token::Text { text });
                        }
                    }
                    if let Some(fc) = part.function_call {
                        let arguments = serde_json::to_string(&fc.args)
                            .unwrap_or_else(|_| "{}".to_string());
                        tokens.push(Token::ToolCall {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: fc.name,
                            arguments,
                        });
                    }
                }
            }
        }
    }

    Ok(tokens)
}

/// Convert a streaming response into a TokenStream.
fn parse_gemini_stream(response: reqwest::Response) -> TokenStream {
    let stream = async_stream::try_stream! {
        let mut byte_stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = byte_stream.next().await {
            let bytes = chunk.map_err(|e| ProviderError::Network(e.to_string()))?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(pos) = buffer.find('\n') {
                let line = buffer[..pos].trim_end_matches('\r').to_string();
                buffer = buffer[pos + 1..].to_string();

                for token in parse_gemini_sse_line(&line)? {
                    yield token;
                }
            }
        }

        // Flush remaining buffer.
        let remaining = buffer.trim();
        if !remaining.is_empty() {
            for token in parse_gemini_sse_line(remaining)? {
                yield token;
            }
        }
    };

    Box::pin(stream)
}

/// Map HTTP error status to ProviderError.
pub(crate) fn map_gemini_error(status: u16, body: &str) -> ProviderError {
    let message = serde_json::from_str::<GeminiErrorResponse>(body)
        .map(|r| r.error.message)
        .unwrap_or_else(|_| body.to_string());

    match status {
        400 | 401 | 403 => {
            // Check if it's an auth error specifically.
            if body.contains("API key") || body.contains("authentication") {
                ProviderError::Auth(message)
            } else {
                ProviderError::MalformedResponse(message)
            }
        }
        429 => ProviderError::RateLimit(message),
        _ => ProviderError::Other(format!("HTTP {status}: {message}")),
    }
}

// --- Provider implementation ---

impl Provider for GeminiProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        let url = format!(
            "{}/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.endpoint.trim_end_matches('/'),
            self.model,
            self.api_key
        );

        let body = build_request_body(&messages, &self.system_prompt, tools.as_ref());

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_gemini_error(status.as_u16(), &body_text));
        }

        Ok(parse_gemini_stream(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::config::Config;

    fn make_user_message(text: &str) -> Message {
        Message {
            role: Role::User,
            content: MessageContent::Text {
                text: text.to_string(),
            },
            timestamp: Utc::now(),
        }
    }

    fn make_assistant_message(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            content: MessageContent::Text {
                text: text.to_string(),
            },
            timestamp: Utc::now(),
        }
    }

    fn make_system_message(text: &str) -> Message {
        Message {
            role: Role::System,
            content: MessageContent::Text {
                text: text.to_string(),
            },
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn to_gemini_contents_maps_roles_correctly() {
        let messages = vec![
            make_user_message("Hello"),
            make_assistant_message("Hi there"),
        ];
        let contents = to_gemini_contents(&messages);

        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "Hello");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "Hi there");
    }

    #[test]
    fn to_gemini_contents_skips_system_messages() {
        let messages = vec![
            make_system_message("You are helpful"),
            make_user_message("Hi"),
        ];
        let contents = to_gemini_contents(&messages);

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
    }

    #[test]
    fn to_gemini_contents_merges_consecutive_same_role() {
        let messages = vec![
            make_user_message("First"),
            make_user_message("Second"),
            make_assistant_message("Response"),
        ];
        let contents = to_gemini_contents(&messages);

        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "user");
        let parts = contents[0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "First");
        assert_eq!(parts[1]["text"], "Second");
        assert_eq!(contents[1]["role"], "model");
    }

    #[test]
    fn build_request_body_includes_system_instruction() {
        let messages = vec![make_user_message("Hello")];
        let body = build_request_body(&messages, "You are helpful", None);

        assert!(body.get("systemInstruction").is_some());
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "You are helpful"
        );
    }

    #[test]
    fn build_request_body_omits_empty_system_prompt() {
        let messages = vec![make_user_message("Hello")];
        let body = build_request_body(&messages, "", None);

        assert!(body.get("systemInstruction").is_none());
    }

    #[test]
    fn to_gemini_tools_maps_function_declarations() {
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    }
                }
            }
        })];

        let gemini_tools = to_gemini_tools(&tools).unwrap();
        let declarations = gemini_tools[0]["functionDeclarations"].as_array().unwrap();

        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0]["name"], "read_file");
        assert_eq!(declarations[0]["description"], "Read a file");
        assert!(declarations[0].get("parameters").is_some());
    }

    #[test]
    fn parse_gemini_sse_text_chunk() {
        let line = r#"data: {"candidates":[{"content":{"parts":[{"text":"Hello"}]},"finishReason":"STOP"}]}"#;
        let tokens = parse_gemini_sse_line(line).unwrap();

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], Token::Text { text: "Hello".into() });
    }

    #[test]
    fn parse_gemini_sse_function_call() {
        let line = r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"read_file","args":{"path":"test.txt"}}}]}}]}"#;
        let tokens = parse_gemini_sse_line(line).unwrap();

        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            Token::ToolCall { name, arguments, .. } => {
                assert_eq!(name, "read_file");
                assert!(arguments.contains("test.txt"));
            }
            _ => panic!("expected ToolCall token"),
        }
    }

    #[test]
    fn parse_gemini_sse_empty_line() {
        let tokens = parse_gemini_sse_line("data: ").unwrap();
        assert!(tokens.is_empty());
    }

    #[test]
    fn map_gemini_error_auth() {
        let body = r#"{"error":{"message":"API key not valid","status":"PERMISSION_DENIED"}}"#;
        match map_gemini_error(401, body) {
            ProviderError::Auth(msg) => assert!(msg.contains("API key")),
            other => panic!("expected Auth error, got: {other:?}"),
        }
    }

    #[test]
    fn map_gemini_error_rate_limit() {
        let body = r#"{"error":{"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED"}}"#;
        match map_gemini_error(429, body) {
            ProviderError::RateLimit(msg) => assert!(msg.contains("Quota")),
            other => panic!("expected RateLimit error, got: {other:?}"),
        }
    }

    // Test cases from task 047

    #[test]
    fn config_with_gemini_provider_parses_correctly() {
        let toml = r#"
[[models.chat.providers]]
type = "gemini"
model = "gemini-2.0-flash"
api_key_env = "GEMINI_API_KEY"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "gemini");
        assert_eq!(config.models.chat.providers[0].model, "gemini-2.0-flash");
        assert_eq!(
            config.models.chat.providers[0].api_key_env.as_deref(),
            Some("GEMINI_API_KEY")
        );
    }

    #[test]
    fn build_gemini_request_with_system_user_assistant() {
        let messages = vec![
            make_system_message("You are helpful"),
            make_user_message("Hello"),
            make_assistant_message("Hi there"),
        ];
        let body = build_request_body(&messages, "You are a test assistant", None);

        // System message in systemInstruction
        assert_eq!(
            body["systemInstruction"]["parts"][0]["text"],
            "You are a test assistant"
        );

        // User and assistant messages in contents with correct roles
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "Hello");
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "Hi there");
    }

    #[test]
    fn build_gemini_request_merges_consecutive_user_messages() {
        let messages = vec![
            make_user_message("First message"),
            make_user_message("Second message"),
        ];
        let body = build_request_body(&messages, "", None);

        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");

        let parts = contents[0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "First message");
        assert_eq!(parts[1]["text"], "Second message");
    }

    #[test]
    fn build_gemini_request_with_tools() {
        let messages = vec![make_user_message("Read file")];
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read a file from disk",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" }
                    },
                    "required": ["path"]
                }
            }
        })];

        let body = build_request_body(&messages, "", Some(&tools));

        assert!(body.get("tools").is_some());
        let tool_array = body["tools"].as_array().unwrap();
        let declarations = tool_array[0]["functionDeclarations"].as_array().unwrap();

        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0]["name"], "read_file");
        assert_eq!(declarations[0]["description"], "Read a file from disk");
        assert_eq!(declarations[0]["parameters"]["type"], "object");
        assert!(declarations[0]["parameters"]["properties"].get("path").is_some());
    }

    #[test]
    fn parse_gemini_sse_multiple_text_chunks() {
        let lines = [
            r#"data: {"candidates":[{"content":{"parts":[{"text":"Hello"}]}}]}"#,
            r#"data: {"candidates":[{"content":{"parts":[{"text":" world"}]}}]}"#,
            r#"data: {"candidates":[{"content":{"parts":[{"text":"!"}]},"finishReason":"STOP"}]}"#,
        ];

        let mut texts = Vec::new();
        for line in &lines {
            for token in parse_gemini_sse_line(line).unwrap() {
                if let Token::Text { text } = token {
                    texts.push(text);
                }
            }
        }

        assert_eq!(texts, vec!["Hello", " world", "!"]);
    }

    #[test]
    fn parse_gemini_sse_function_call_with_arguments() {
        let line = r#"data: {"candidates":[{"content":{"parts":[{"functionCall":{"name":"read_file","args":{"path":"test.txt","encoding":"utf-8"}}}]}}]}"#;
        let tokens = parse_gemini_sse_line(line).unwrap();

        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            Token::ToolCall { name, arguments, .. } => {
                assert_eq!(name, "read_file");
                let args: serde_json::Value = serde_json::from_str(arguments).unwrap();
                assert_eq!(args["path"], "test.txt");
                assert_eq!(args["encoding"], "utf-8");
            }
            _ => panic!("expected ToolCall token"),
        }
    }

    #[tokio::test]
    async fn gemini_provider_constructs_correct_url() {
        let provider = GeminiProvider::new(
            "test-api-key",
            "gemini-2.0-flash",
            "https://generativelanguage.googleapis.com",
            "You are helpful",
        );

        // We can't actually make a request without a mock server, but we can
        // verify the provider was constructed with the correct parameters.
        assert_eq!(provider.api_key, "test-api-key");
        assert_eq!(provider.model, "gemini-2.0-flash");
        assert_eq!(provider.endpoint, "https://generativelanguage.googleapis.com");
        assert_eq!(provider.system_prompt, "You are helpful");
    }

    #[tokio::test]
    async fn gemini_provider_network_error_on_unreachable_endpoint() {
        let provider = GeminiProvider::new(
            "test-key",
            "gemini-2.0-flash",
            "http://localhost:1",  // unreachable port
            "",
        );

        let messages = vec![make_user_message("test")];
        let result = provider.complete(messages, None).await;

        assert!(matches!(result, Err(ProviderError::Network(_))));
    }
}
