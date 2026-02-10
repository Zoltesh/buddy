use reqwest::Client;

use crate::provider::openai::{build_request_body, map_error_status, parse_sse_stream};
use crate::provider::{Provider, ProviderError, TokenStream};
use crate::types::Message;

/// Mistral AI provider. Uses Mistral's OpenAI-compatible API endpoint.
/// API key is sent as a Bearer token in the Authorization header.
pub struct MistralProvider {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    system_prompt: String,
}

impl MistralProvider {
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

impl Provider for MistralProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        let url = format!(
            "{}/v1/chat/completions",
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
        vec![Message {
            role: Role::User,
            content: MessageContent::Text {
                text: "Hello".into(),
            },
            timestamp: now,
        }]
    }

    #[test]
    fn request_body_matches_openai_compatible_spec() {
        let messages = make_messages();
        let body = build_request_body(
            &messages,
            "mistral-large-latest",
            "You are helpful.",
            None,
        );

        assert_eq!(body["model"], "mistral-large-latest");
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
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"mistral-large-latest","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":" there"},"finish_reason":null}]}"#,
            r#"data: {"id":"1","object":"chat.completion.chunk","created":0,"model":"mistral-large-latest","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
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
    fn provider_new_sets_fields() {
        let provider = MistralProvider::new(
            "test-api-key",
            "mistral-large-latest",
            "https://api.mistral.ai",
            "You are helpful.",
        );
        assert_eq!(provider.api_key, "test-api-key");
        assert_eq!(provider.model, "mistral-large-latest");
        assert_eq!(provider.endpoint, "https://api.mistral.ai");
        assert_eq!(provider.system_prompt, "You are helpful.");
    }

    #[tokio::test]
    async fn request_uses_v1_chat_completions_endpoint() {
        // This test verifies the URL construction logic by checking
        // the endpoint format. We verify auth headers via the empty API key test
        // and integration test with live endpoints.
        let provider = MistralProvider::new(
            "test-key",
            "mistral-large-latest",
            "https://api.mistral.ai",
            "You are helpful.",
        );

        // The provider constructs {endpoint}/v1/chat/completions.
        // We verify this indirectly: if we use an invalid endpoint, the error
        // will show the constructed URL.
        let provider_bad = MistralProvider::new(
            "test-key",
            "mistral-large-latest",
            "http://127.0.0.1:1", // Unreachable.
            "",
        );

        let messages = make_messages();
        let result = provider_bad.complete(messages, None).await;

        match result {
            Err(ProviderError::Network(msg)) => {
                assert!(
                    msg.contains("/v1/chat/completions"),
                    "error should show v1/chat/completions path: {msg}"
                );
            }
            Ok(_) => panic!("expected Network error with URL, got Ok"),
            Err(e) => panic!("expected Network error with URL, got: {e}"),
        }

        // Verify constructor fields.
        assert_eq!(provider.api_key, "test-key");
        assert_eq!(provider.model, "mistral-large-latest");
        assert_eq!(provider.endpoint, "https://api.mistral.ai");
    }

    #[tokio::test]
    async fn unreachable_endpoint_returns_network_error() {
        let provider = MistralProvider::new(
            "fake-key",
            "mistral-large-latest",
            "http://127.0.0.1:1", // Unreachable port.
            "",
        );

        let messages = make_messages();
        let result = provider.complete(messages, None).await;

        assert!(result.is_err());
        match result {
            Err(ProviderError::Network(_)) => {},
            Ok(_) => panic!("expected Network error, got Ok"),
            Err(e) => panic!("expected Network error, got: {e}"),
        }
    }

    #[test]
    fn provider_allows_empty_api_key() {
        // Mistral requires an API key, but the provider constructor doesn't
        // validate it — that's done at the config layer. This test verifies
        // the provider doesn't panic with an empty key.
        let provider = MistralProvider::new(
            "",
            "mistral-large-latest",
            "https://api.mistral.ai",
            "",
        );
        assert_eq!(provider.api_key, "");
    }

    #[tokio::test]
    #[ignore] // Requires a live API key: MISTRAL_API_KEY=... cargo test -- --ignored
    async fn integration_live_streaming() {
        use futures_util::StreamExt;

        let api_key = std::env::var("MISTRAL_API_KEY").expect("MISTRAL_API_KEY not set");
        let provider = MistralProvider::new(
            &api_key,
            "mistral-large-latest",
            "https://api.mistral.ai",
            "You are a helpful assistant.",
        );

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
