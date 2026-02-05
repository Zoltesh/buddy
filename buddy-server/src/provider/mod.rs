pub mod openai;

use std::future::Future;
use std::pin::Pin;

use futures_core::Stream;

use crate::types::Message;

/// A chunk of streamed LLM output.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
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
    ) -> impl Future<Output = Result<TokenStream, ProviderError>> + Send;
}
