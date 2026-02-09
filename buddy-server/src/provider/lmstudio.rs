use reqwest::Client;

use crate::provider::openai::{build_request_body, map_error_status, parse_sse_stream};
use crate::provider::{Provider, ProviderError, TokenStream};
use crate::types::Message;

/// LM Studio provider. Connects to a local LM Studio server using its
/// OpenAI-compatible chat completions endpoint. No API key required.
pub struct LmStudioProvider {
    client: Client,
    model: String,
    endpoint: String,
    system_prompt: String,
}

impl LmStudioProvider {
    pub fn new(model: &str, endpoint: &str, system_prompt: &str) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build HTTP client"),
            model: model.to_string(),
            endpoint: endpoint.to_string(),
            system_prompt: system_prompt.to_string(),
        }
    }
}

impl Provider for LmStudioProvider {
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
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(map_error_status(status.as_u16(), &body_text));
        }

        Ok(parse_sse_stream(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::openai::{
        build_request_body, map_error_status, parse_sse_line, SseChunk, ToolCallAccumulator,
    };
    use crate::provider::{ProviderError, Token};
    use crate::types::{Message, MessageContent, Role};
    use chrono::Utc;

    fn make_messages() -> Vec<Message> {
        let now = Utc::now();
        vec![
            Message {
                role: Role::User,
                content: MessageContent::Text {
                    text: "Hello".into(),
                },
                timestamp: now,
            },
        ]
    }

    #[test]
    fn request_body_matches_openai_compatible_spec() {
        let messages = make_messages();
        let body = build_request_body(&messages, "deepseek-coder", "You are helpful.", None);

        assert_eq!(body["model"], "deepseek-coder");
        assert_eq!(body["stream"], true);

        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"], "Hello");
    }

    #[test]
    fn parse_streaming_text_tokens() {
        let lines = [
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"deepseek-coder","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"deepseek-coder","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"deepseek-coder","choices":[{"index":0,"delta":{"content":" there"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"deepseek-coder","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
            "data: [DONE]",
        ];

        let mut texts = Vec::new();
        for line in &lines {
            if let SseChunk::TextDelta(t) = parse_sse_line(line).unwrap() {
                texts.push(t);
            }
        }
        assert_eq!(texts, vec!["Hi", " there"]);
    }

    #[test]
    fn parse_tool_call_response() {
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
    fn error_status_401_maps_to_auth() {
        let body = r#"{"error":{"message":"Unauthorized","type":"auth_error","code":"unauthorized"}}"#;
        match map_error_status(401, body) {
            ProviderError::Auth(msg) => assert_eq!(msg, "Unauthorized"),
            other => panic!("expected Auth, got: {other:?}"),
        }
    }

    #[test]
    fn error_status_429_maps_to_rate_limit() {
        let body = r#"{"error":{"message":"Too many requests","type":"rate_limit","code":"rate_limit"}}"#;
        match map_error_status(429, body) {
            ProviderError::RateLimit(msg) => assert_eq!(msg, "Too many requests"),
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
    fn provider_new_sets_fields() {
        let provider = LmStudioProvider::new(
            "deepseek-coder",
            "http://192.168.1.100:1234/v1",
            "You are helpful.",
        );
        assert_eq!(provider.model, "deepseek-coder");
        assert_eq!(provider.endpoint, "http://192.168.1.100:1234/v1");
    }

    #[tokio::test]
    #[ignore] // Requires a running LM Studio server: LMSTUDIO_ENDPOINT=... cargo test -- --ignored
    async fn integration_live_streaming() {
        use futures_util::StreamExt;

        let endpoint = std::env::var("LMSTUDIO_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:1234/v1".into());
        let model =
            std::env::var("LMSTUDIO_MODEL").unwrap_or_else(|_| "deepseek-coder".into());

        let provider = LmStudioProvider::new(&model, &endpoint, "You are a helpful assistant.");

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
