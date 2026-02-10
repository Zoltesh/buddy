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

mod chat;
mod config;
mod conversation;
mod embedder;
mod memory;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};

use crate::provider::Provider;

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
pub use memory::{clear_memory, get_memory_status, migrate_memory};

// ── Shared types ────────────────────────────────────────────────────────

/// Incoming chat request.
#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub messages: Vec<crate::types::Message>,
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
    Warnings { warnings: Vec<crate::warning::Warning> },
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

/// Pending approval requests awaiting user response.
pub type PendingApprovals = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// Skills already approved once per conversation (`once` policy).
pub type ConversationApprovals = Arc<Mutex<HashMap<String, HashSet<String>>>>;

/// Create a new empty `PendingApprovals` map.
pub fn new_pending_approvals() -> PendingApprovals {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Request body for `POST /api/chat/{conversation_id}/approve`.
#[derive(Deserialize)]
pub struct ApproveRequest {
    pub approval_id: String,
    pub approved: bool,
}

/// Shared application state.
///
/// Fields wrapped in `ArcSwap` are hot-reloadable: they can be atomically
/// replaced when the configuration changes without interrupting in-flight
/// requests. Handlers call `.load()` to get a snapshot for the duration of
/// the request.
pub struct AppState<P> {
    pub provider: arc_swap::ArcSwap<P>,
    pub registry: arc_swap::ArcSwap<crate::skill::SkillRegistry>,
    pub store: crate::store::Store,
    pub embedder: arc_swap::ArcSwap<Option<std::sync::Arc<dyn crate::embedding::Embedder>>>,
    pub vector_store: arc_swap::ArcSwap<Option<std::sync::Arc<dyn crate::memory::VectorStore>>>,
    pub working_memory: crate::skill::working_memory::WorkingMemoryMap,
    pub memory_config: arc_swap::ArcSwap<crate::config::MemoryConfig>,
    pub warnings: crate::warning::SharedWarnings,
    pub pending_approvals: PendingApprovals,
    pub conversation_approvals: ConversationApprovals,
    pub approval_overrides: arc_swap::ArcSwap<HashMap<String, crate::config::ApprovalPolicy>>,
    pub approval_timeout: std::time::Duration,
    pub config: std::sync::RwLock<crate::config::Config>,
    pub config_path: std::path::PathBuf,
    /// Optional callback invoked after a successful config write to hot-reload
    /// runtime components. Set to `Some` in production (via `reload::reload_from_config`),
    /// left as `None` in tests that don't need reload behavior.
    pub on_config_change: Option<Box<dyn Fn(&Self) -> Result<(), String> + Send + Sync>>,
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
) -> Json<Vec<crate::warning::Warning>> {
    let collector = state.warnings.read().unwrap();
    Json(collector.list().to_vec())
}
