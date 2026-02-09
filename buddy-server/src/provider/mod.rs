pub mod lmstudio;
pub mod openai;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures_core::Stream;
use futures_util::StreamExt;

use crate::types::Message;

/// A chunk of streamed LLM output.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A text content delta.
    Text { text: String },
    /// The LLM is requesting a tool execution.
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    /// A non-fatal warning (e.g. provider fallback occurred).
    Warning { message: String },
}

/// Errors that can occur when calling an LLM provider.
#[derive(Debug)]
pub enum ProviderError {
    /// Network-level failure (DNS, timeout, connection reset, etc.)
    Network(String),
    /// Authentication failure (invalid or expired API key)
    Auth(String),
    /// Rate limit exceeded
    RateLimit(String),
    /// Response could not be parsed
    MalformedResponse(String),
    /// Any other error
    Other(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::Auth(msg) => write!(f, "auth error: {msg}"),
            Self::RateLimit(msg) => write!(f, "rate limited: {msg}"),
            Self::MalformedResponse(msg) => write!(f, "malformed response: {msg}"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// A stream of tokens from an LLM provider.
pub type TokenStream = Pin<Box<dyn Stream<Item = Result<Token, ProviderError>> + Send>>;

/// Trait abstracting LLM interaction.
pub trait Provider: Send + Sync {
    fn complete(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> impl Future<Output = Result<TokenStream, ProviderError>> + Send;
}

/// Enum dispatch over all supported providers. This avoids the need for
/// `dyn Provider` (which is not object-safe due to `impl Future` return)
/// while keeping `main.rs` free of generics.
pub enum AnyProvider {
    OpenAi(openai::OpenAiProvider),
    LmStudio(lmstudio::LmStudioProvider),
}

impl Provider for AnyProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        match self {
            Self::OpenAi(p) => p.complete(messages, tools).await,
            Self::LmStudio(p) => p.complete(messages, tools).await,
        }
    }
}

/// Ordered list of providers with automatic fallback on transient errors.
///
/// Tries providers in order. On `Network` or `RateLimit` errors, advances to
/// the next provider. On `Auth` or `MalformedResponse` errors, stops
/// immediately (configuration problems should not be masked). If all providers
/// fail, returns the last error.
pub struct ProviderChain<P> {
    providers: Vec<(P, String)>,
    /// Index of the last provider that completed successfully. Subsequent
    /// requests start here to avoid repeatedly timing out on a known-bad
    /// provider.
    last_ok: AtomicUsize,
}

impl<P: Provider> ProviderChain<P> {
    pub fn new(providers: Vec<(P, String)>) -> Self {
        assert!(!providers.is_empty(), "ProviderChain requires at least one provider");
        Self {
            providers,
            last_ok: AtomicUsize::new(0),
        }
    }

    /// Returns the number of providers in the chain.
    pub fn len(&self) -> usize {
        self.providers.len()
    }
}

impl<P: Provider> Provider for ProviderChain<P> {
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<serde_json::Value>>,
    ) -> Result<TokenStream, ProviderError> {
        let start = self.last_ok.load(Ordering::Relaxed);

        // Try the last-known-good provider first.
        let (ref provider, ref name) = self.providers[start];
        let mut last_error = match provider.complete(messages.clone(), tools.clone()).await {
            Ok(stream) => return Ok(stream),
            Err(e) => match &e {
                ProviderError::Network(_) | ProviderError::RateLimit(_) => {
                    eprintln!("Provider {start} ({name}) failed: {e}, trying next");
                    e
                }
                _ => return Err(e),
            },
        };

        // Last-known-good failed — try remaining providers in order, skipping
        // the one we already tried.
        for (i, (provider, name)) in self.providers.iter().enumerate() {
            if i == start {
                continue;
            }
            match provider.complete(messages.clone(), tools.clone()).await {
                Ok(stream) => {
                    self.last_ok.store(i, Ordering::Relaxed);
                    if i > 0 {
                        let warning_msg =
                            format!("Primary model unavailable, using fallback: {name}");
                        eprintln!("{warning_msg}");
                        let warning_stream = futures_util::stream::once(async move {
                            Ok(Token::Warning { message: warning_msg })
                        });
                        return Ok(Box::pin(warning_stream.chain(stream)));
                    }
                    return Ok(stream);
                }
                Err(e) => match &e {
                    ProviderError::Network(_) | ProviderError::RateLimit(_) => {
                        eprintln!("Provider {i} ({name}) failed: {e}, trying next");
                        last_error = e;
                    }
                    _ => return Err(e),
                },
            }
        }

        Err(last_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SuccessMock {
        tokens: Vec<String>,
    }

    impl Provider for SuccessMock {
        async fn complete(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<serde_json::Value>>,
        ) -> Result<TokenStream, ProviderError> {
            let tokens = self.tokens.clone();
            let stream = async_stream::try_stream! {
                for text in tokens {
                    yield Token::Text { text };
                }
            };
            Ok(Box::pin(stream))
        }
    }

    struct NetworkFailMock;

    impl Provider for NetworkFailMock {
        async fn complete(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<serde_json::Value>>,
        ) -> Result<TokenStream, ProviderError> {
            Err(ProviderError::Network("connection refused".into()))
        }
    }

    struct RateLimitFailMock;

    impl Provider for RateLimitFailMock {
        async fn complete(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<serde_json::Value>>,
        ) -> Result<TokenStream, ProviderError> {
            Err(ProviderError::RateLimit("429 too many requests".into()))
        }
    }

    struct AuthFailMock;

    impl Provider for AuthFailMock {
        async fn complete(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<serde_json::Value>>,
        ) -> Result<TokenStream, ProviderError> {
            Err(ProviderError::Auth("invalid api key".into()))
        }
    }

    // ProviderChain needs all elements to be the same type. Use an enum to
    // allow mixing different behaviours in a single chain.
    enum FlexMock {
        Success(SuccessMock),
        NetworkFail(NetworkFailMock),
        RateLimitFail(RateLimitFailMock),
        AuthFail(AuthFailMock),
    }

    impl Provider for FlexMock {
        async fn complete(
            &self,
            messages: Vec<Message>,
            tools: Option<Vec<serde_json::Value>>,
        ) -> Result<TokenStream, ProviderError> {
            match self {
                Self::Success(p) => p.complete(messages, tools).await,
                Self::NetworkFail(p) => p.complete(messages, tools).await,
                Self::RateLimitFail(p) => p.complete(messages, tools).await,
                Self::AuthFail(p) => p.complete(messages, tools).await,
            }
        }
    }

    fn flex_success(tokens: Vec<&str>) -> (FlexMock, String) {
        (
            FlexMock::Success(SuccessMock {
                tokens: tokens.into_iter().map(String::from).collect(),
            }),
            "success-model".into(),
        )
    }

    fn flex_network_fail() -> (FlexMock, String) {
        (FlexMock::NetworkFail(NetworkFailMock), "network-fail-model".into())
    }

    fn flex_rate_limit_fail() -> (FlexMock, String) {
        (FlexMock::RateLimitFail(RateLimitFailMock), "ratelimit-fail-model".into())
    }

    fn flex_auth_fail() -> (FlexMock, String) {
        (FlexMock::AuthFail(AuthFailMock), "auth-fail-model".into())
    }

    /// Consume a TokenStream and return all tokens.
    async fn collect_tokens(stream: TokenStream) -> Vec<Token> {
        tokio::pin!(stream);
        let mut tokens = Vec::new();
        while let Some(result) = stream.next().await {
            tokens.push(result.unwrap());
        }
        tokens
    }

    #[tokio::test]
    async fn fallback_on_network_error() {
        let chain = ProviderChain::new(vec![
            flex_network_fail(),
            flex_success(vec!["hello"]),
        ]);
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;

        // Should have a warning followed by the text.
        assert!(matches!(&tokens[0], Token::Warning { message } if message.contains("fallback")));
        assert_eq!(tokens[1], Token::Text { text: "hello".into() });
    }

    #[tokio::test]
    async fn fallback_on_rate_limit_error() {
        let chain = ProviderChain::new(vec![
            flex_rate_limit_fail(),
            flex_success(vec!["ok"]),
        ]);
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;

        assert!(matches!(&tokens[0], Token::Warning { .. }));
        assert_eq!(tokens[1], Token::Text { text: "ok".into() });
    }

    #[tokio::test]
    async fn auth_error_not_retried() {
        let chain = ProviderChain::new(vec![
            flex_auth_fail(),
            flex_success(vec!["should not reach"]),
        ]);
        let result = chain.complete(vec![], None).await;

        assert!(matches!(result, Err(ProviderError::Auth(_))));
    }

    #[tokio::test]
    async fn three_providers_first_two_fail() {
        let chain = ProviderChain::new(vec![
            flex_network_fail(),
            flex_network_fail(),
            flex_success(vec!["third"]),
        ]);
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;

        assert!(matches!(&tokens[0], Token::Warning { message } if message.contains("success-model")));
        assert_eq!(tokens[1], Token::Text { text: "third".into() });
    }

    #[tokio::test]
    async fn all_providers_fail_returns_last_error() {
        let chain = ProviderChain::new(vec![
            flex_network_fail(),
            flex_network_fail(),
        ]);
        let result = chain.complete(vec![], None).await;

        assert!(matches!(result, Err(ProviderError::Network(_))));
    }

    #[tokio::test]
    async fn single_provider_success_no_warning() {
        let chain = ProviderChain::new(vec![
            flex_success(vec!["solo"]),
        ]);
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;

        // No warning token — just the text.
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], Token::Text { text: "solo".into() });
    }

    #[tokio::test]
    async fn sticky_fallback_skips_failed_provider() {
        let chain = ProviderChain::new(vec![
            flex_network_fail(),
            flex_success(vec!["hello"]),
        ]);

        // First call: falls back to provider 1, emitting a warning.
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;
        assert!(matches!(&tokens[0], Token::Warning { .. }));
        assert_eq!(tokens[1], Token::Text { text: "hello".into() });

        // Second call: starts at provider 1 directly — no warning.
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], Token::Text { text: "hello".into() });
    }

    #[tokio::test]
    async fn sticky_recovers_to_primary() {
        // Both succeed, but start sticky on index 1 to simulate recovery.
        let chain = ProviderChain::new(vec![
            flex_success(vec!["primary"]),
            flex_success(vec!["fallback"]),
        ]);

        // Force sticky to index 1 (simulating a previous fallback).
        chain.last_ok.store(1, std::sync::atomic::Ordering::Relaxed);

        // Provider 1 works — no warning, stays sticky.
        let stream = chain.complete(vec![], None).await.unwrap();
        let tokens = collect_tokens(stream).await;
        assert_eq!(tokens, vec![Token::Text { text: "fallback".into() }]);
    }
}
