//! HTTP API layer for buddy.
//!
//! ## Streaming transport: Server-Sent Events (SSE)
//!
//! V0.1 uses SSE via `POST /api/chat` for streaming responses. The client
//! sends a JSON `ChatRequest` and receives a stream of `ChatEvent` frames.
//!
//! SSE was chosen over WebSocket for V0.1 because:
//! - Standard HTTP semantics — malformed requests get proper 4xx status codes
//!   before any streaming begins
//! - Simpler client implementation (fetch + EventSource parsing)
//! - Works transparently with HTTP proxies and load balancers
//! - WebSocket can be added later if bidirectional communication is needed

pub mod auth;
mod chat;
mod config;
mod conversation;
mod embedder;
mod interfaces;
mod memory;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use buddy_core::provider::Provider;
pub use buddy_core::state::{AppState, ConversationApprovals, PendingApprovals, new_pending_approvals};

// Re-export handler functions for use in main.rs router setup.
pub use chat::{approve_handler, chat_handler};
pub use config::{
    discover_models, get_config, put_config_chat, put_config_memory, put_config_models,
    put_config_server, put_config_skills, test_provider,
};
pub use conversation::{
    create_conversation, delete_conversation, get_conversation, list_conversations,
};
pub use embedder::get_embedder_health;
pub use interfaces::{get_interfaces_status, put_config_interfaces};
pub use memory::{clear_memory, get_memory_status, migrate_memory};

// ── Shared types ────────────────────────────────────────────────────────

/// Incoming chat request.
#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub messages: Vec<buddy_core::types::Message>,
    #[serde(default)]
    pub disable_memory: bool,
}

/// A recalled memory snippet surfaced to the frontend.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct MemorySnippet {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub score: f32,
}

/// A single frame in the streamed response.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    ConversationMeta { conversation_id: String },
    Warnings { warnings: Vec<buddy_core::warning::Warning> },
    Warning { message: String },
    MemoryContext { memories: Vec<MemorySnippet> },
    TokenDelta { content: String },
    ToolCallStart { id: String, name: String, arguments: String },
    ToolCallResult { id: String, content: String },
    ApprovalRequest { id: String, skill_name: String, arguments: serde_json::Value, permission_level: String },
    Done,
    Error { message: String },
}

/// Structured API error response.
#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Request body for `POST /api/chat/{conversation_id}/approve`.
#[derive(Deserialize)]
pub struct ApproveRequest {
    pub approval_id: String,
    pub approved: bool,
}

// ── Error helpers ───────────────────────────────────────────────────────

pub(crate) fn internal_error(message: String) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            code: "internal_error".into(),
            message,
        }),
    )
}

pub(crate) fn not_found_error(message: String) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            code: "not_found".into(),
            message,
        }),
    )
}

// ── Warnings endpoint ───────────────────────────────────────────────────

/// `GET /api/warnings` — return current system warnings.
pub async fn get_warnings<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<Vec<buddy_core::warning::Warning>> {
    let collector = state.warnings.read().unwrap();
    Json(collector.list().to_vec())
}
