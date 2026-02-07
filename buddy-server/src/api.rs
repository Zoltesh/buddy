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

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{oneshot, Mutex};

use crate::config::ApprovalPolicy;
use crate::provider::{Provider, Token};
use crate::skill::PermissionLevel;
use crate::store::title_from_message;
use crate::types::{Message, MessageContent, Role};

/// Maximum number of tool-call loop iterations before aborting.
const MAX_TOOL_ITERATIONS: usize = 10;

/// Incoming chat request.
#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub messages: Vec<Message>,
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
    pub approval_overrides: arc_swap::ArcSwap<HashMap<String, ApprovalPolicy>>,
    pub approval_timeout: std::time::Duration,
    pub config: std::sync::RwLock<crate::config::Config>,
    pub config_path: std::path::PathBuf,
    /// Optional callback invoked after a successful config write to hot-reload
    /// runtime components. Set to `Some` in production (via `reload::reload_from_config`),
    /// left as `None` in tests that don't need reload behavior.
    pub on_config_change: Option<Box<dyn Fn(&Self) -> Result<(), String> + Send + Sync>>,
}

// ── Conversation CRUD handlers ──────────────────────────────────────────

/// `GET /api/conversations` — list all conversations.
pub async fn list_conversations<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<Vec<crate::store::ConversationSummary>>, (StatusCode, Json<ApiError>)> {
    let list = state.store.list_conversations().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    Ok(Json(list))
}

/// `POST /api/conversations` — create a new empty conversation.
pub async fn create_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<(StatusCode, Json<crate::store::Conversation>), (StatusCode, Json<ApiError>)> {
    let conv = state.store.create_conversation("New conversation").map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    Ok((StatusCode::CREATED, Json(conv)))
}

/// `GET /api/conversations/:id` — get a single conversation with all messages.
pub async fn get_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(id): Path<String>,
) -> Result<Json<crate::store::Conversation>, (StatusCode, Json<ApiError>)> {
    let conv = state.store.get_conversation(&id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    match conv {
        Some(c) => Ok(Json(c)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: "not_found".into(),
                message: format!("conversation '{id}' not found"),
            }),
        )),
    }
}

/// `DELETE /api/conversations/:id` — delete a conversation and all messages.
pub async fn delete_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let deleted = state.store.delete_conversation(&id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: e,
            }),
        )
    })?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: "not_found".into(),
                message: format!("conversation '{id}' not found"),
            }),
        ))
    }
}

// ── Warnings endpoint ───────────────────────────────────────────────────

/// `GET /api/warnings` — return current system warnings.
pub async fn get_warnings<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<Vec<crate::warning::Warning>> {
    let collector = state.warnings.read().unwrap();
    Json(collector.list().to_vec())
}

// ── Config handlers ─────────────────────────────────────────────────────

/// `GET /api/config` — return the current configuration as JSON.
pub async fn get_config<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<crate::config::Config> {
    let config = state.config.read().unwrap();
    Json(config.clone())
}

// ── Config write types and helpers ───────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidationErrorResponse {
    pub errors: Vec<FieldError>,
}

fn validate_models(models: &crate::config::ModelsConfig) -> Vec<FieldError> {
    let mut errors = Vec::new();
    if models.chat.providers.is_empty() {
        errors.push(FieldError {
            field: "models.chat.providers".into(),
            message: "must not be empty".into(),
        });
    }
    for (i, p) in models.chat.providers.iter().enumerate() {
        if !["openai", "lmstudio", "local"].contains(&p.provider_type.as_str()) {
            errors.push(FieldError {
                field: format!("models.chat.providers[{i}].type"),
                message: format!(
                    "unknown provider type '{}'; expected openai, lmstudio, or local",
                    p.provider_type
                ),
            });
        }
        if p.model.is_empty() {
            errors.push(FieldError {
                field: format!("models.chat.providers[{i}].model"),
                message: "must not be empty".into(),
            });
        }
    }
    if let Some(ref emb) = models.embedding {
        for (i, p) in emb.providers.iter().enumerate() {
            if !["openai", "lmstudio", "local"].contains(&p.provider_type.as_str()) {
                errors.push(FieldError {
                    field: format!("models.embedding.providers[{i}].type"),
                    message: format!(
                        "unknown provider type '{}'; expected openai, lmstudio, or local",
                        p.provider_type
                    ),
                });
            }
            if p.model.is_empty() {
                errors.push(FieldError {
                    field: format!("models.embedding.providers[{i}].model"),
                    message: "must not be empty".into(),
                });
            }
        }
    }
    errors
}

fn validate_server(server: &crate::config::ServerConfig) -> Vec<FieldError> {
    let mut errors = Vec::new();
    if server.port == 0 {
        errors.push(FieldError {
            field: "server.port".into(),
            message: "must be between 1 and 65535".into(),
        });
    }
    errors
}

fn validate_skills(skills: &crate::config::SkillsConfig) -> Vec<FieldError> {
    let mut errors = Vec::new();
    if let Some(ref rf) = skills.read_file {
        for (i, dir) in rf.allowed_directories.iter().enumerate() {
            let path = std::path::Path::new(dir);
            if !path.is_dir() {
                errors.push(FieldError {
                    field: format!("skills.read_file.allowed_directories[{i}]"),
                    message: format!("'{}' does not exist or is not a directory", dir),
                });
            }
        }
    }
    if let Some(ref wf) = skills.write_file {
        for (i, dir) in wf.allowed_directories.iter().enumerate() {
            let path = std::path::Path::new(dir);
            if !path.is_dir() {
                errors.push(FieldError {
                    field: format!("skills.write_file.allowed_directories[{i}]"),
                    message: format!("'{}' does not exist or is not a directory", dir),
                });
            }
        }
    }
    if let Some(ref fu) = skills.fetch_url {
        for (i, domain) in fu.allowed_domains.iter().enumerate() {
            if domain.is_empty() {
                errors.push(FieldError {
                    field: format!("skills.fetch_url.allowed_domains[{i}]"),
                    message: "must not be empty".into(),
                });
            }
        }
    }
    errors
}

fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "config path has no parent directory".to_string())?;
    let tmp_path = parent.join(".buddy.toml.tmp");
    std::fs::write(&tmp_path, content)
        .map_err(|e| format!("failed to write temp file: {e}"))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("failed to rename temp file: {e}"))?;
    Ok(())
}

// ── Config write handlers ───────────────────────────────────────────────

/// `PUT /api/config/models` — update the models section.
pub async fn put_config_models<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(models): Json<crate::config::ModelsConfig>,
) -> axum::response::Response {
    let errors = validate_models(&models);
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ValidationErrorResponse { errors })).into_response();
    }
    {
        let mut config = state.config.write().unwrap();
        config.models = models;
        let toml = config.to_toml_string();
        if let Err(e) = atomic_write(&state.config_path, &toml) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "write_error".into(), message: e }),
            ).into_response();
        }
    }
    if let Some(ref hook) = state.on_config_change {
        if let Err(e) = hook(&state) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "reload_failed".into(), message: format!("config saved but reload failed: {e}") }),
            ).into_response();
        }
    }
    let config = state.config.read().unwrap();
    Json(config.clone()).into_response()
}

/// `PUT /api/config/skills` — update the skills section.
pub async fn put_config_skills<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(skills): Json<crate::config::SkillsConfig>,
) -> axum::response::Response {
    let errors = validate_skills(&skills);
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ValidationErrorResponse { errors })).into_response();
    }
    {
        let mut config = state.config.write().unwrap();
        config.skills = skills;
        let toml = config.to_toml_string();
        if let Err(e) = atomic_write(&state.config_path, &toml) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "write_error".into(), message: e }),
            ).into_response();
        }
    }
    if let Some(ref hook) = state.on_config_change {
        if let Err(e) = hook(&state) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "reload_failed".into(), message: format!("config saved but reload failed: {e}") }),
            ).into_response();
        }
    }
    let config = state.config.read().unwrap();
    Json(config.clone()).into_response()
}

/// `PUT /api/config/chat` — update the chat section.
pub async fn put_config_chat<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(chat): Json<crate::config::ChatConfig>,
) -> axum::response::Response {
    {
        let mut config = state.config.write().unwrap();
        config.chat = chat;
        let toml = config.to_toml_string();
        if let Err(e) = atomic_write(&state.config_path, &toml) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "write_error".into(), message: e }),
            ).into_response();
        }
    }
    if let Some(ref hook) = state.on_config_change {
        if let Err(e) = hook(&state) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "reload_failed".into(), message: format!("config saved but reload failed: {e}") }),
            ).into_response();
        }
    }
    let config = state.config.read().unwrap();
    Json(config.clone()).into_response()
}

/// Response wrapper for config changes that may require a server restart.
#[derive(Serialize)]
struct ConfigWithNotes {
    #[serde(flatten)]
    config: crate::config::Config,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
}

/// `PUT /api/config/server` — update the server section.
///
/// Server bind address changes (`host`, `port`) are persisted but require a
/// restart to take effect. The response includes a note indicating this.
pub async fn put_config_server<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(server): Json<crate::config::ServerConfig>,
) -> axum::response::Response {
    let errors = validate_server(&server);
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ValidationErrorResponse { errors })).into_response();
    }
    {
        let mut config = state.config.write().unwrap();
        config.server = server;
        let toml = config.to_toml_string();
        if let Err(e) = atomic_write(&state.config_path, &toml) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "write_error".into(), message: e }),
            ).into_response();
        }
    }
    // Server config changes require a restart — add warning and response note.
    {
        let mut collector = state.warnings.write().unwrap();
        collector.clear("restart_required");
        collector.add(crate::warning::Warning {
            code: "restart_required".into(),
            message: "Server config changed — restart required for bind address changes to take effect.".into(),
            severity: crate::warning::WarningSeverity::Warning,
        });
    }
    let config = state.config.read().unwrap();
    Json(ConfigWithNotes {
        config: config.clone(),
        notes: vec!["Server config changes require a restart to take effect.".into()],
    }).into_response()
}

/// `PUT /api/config/memory` — update the memory section.
pub async fn put_config_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(memory): Json<crate::config::MemoryConfig>,
) -> axum::response::Response {
    {
        let mut config = state.config.write().unwrap();
        config.memory = memory;
        let toml = config.to_toml_string();
        if let Err(e) = atomic_write(&state.config_path, &toml) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "write_error".into(), message: e }),
            ).into_response();
        }
    }
    if let Some(ref hook) = state.on_config_change {
        if let Err(e) = hook(&state) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "reload_failed".into(), message: format!("config saved but reload failed: {e}") }),
            ).into_response();
        }
    }
    let config = state.config.read().unwrap();
    Json(config.clone()).into_response()
}

// ── Provider connection test ────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct TestProviderResponse {
    pub status: String,
    pub message: String,
}

/// `POST /api/config/test-provider` — dry-run connectivity check for a provider.
pub async fn test_provider<P: Provider + 'static>(
    State(_state): State<Arc<AppState<P>>>,
    Json(entry): Json<crate::config::ProviderEntry>,
) -> axum::response::Response {
    // Validate provider type.
    if !["openai", "lmstudio", "local"].contains(&entry.provider_type.as_str()) {
        let errors = vec![FieldError {
            field: "type".into(),
            message: format!(
                "unknown provider type '{}'; expected openai, lmstudio, or local",
                entry.provider_type
            ),
        }];
        return (StatusCode::BAD_REQUEST, Json(ValidationErrorResponse { errors }))
            .into_response();
    }

    // Validate model is not empty.
    if entry.model.is_empty() {
        let errors = vec![FieldError {
            field: "model".into(),
            message: "must not be empty".into(),
        }];
        return (StatusCode::BAD_REQUEST, Json(ValidationErrorResponse { errors }))
            .into_response();
    }

    // Resolve API key from env.
    let api_key = match entry.resolve_api_key() {
        Ok(key) => key,
        Err(msg) => {
            return Json(TestProviderResponse {
                status: "error".into(),
                message: msg,
            })
            .into_response();
        }
    };

    // OpenAI type requires an API key.
    if entry.provider_type == "openai" && api_key.is_empty() {
        return Json(TestProviderResponse {
            status: "error".into(),
            message: "api_key_env is required when type = \"openai\"".into(),
        })
        .into_response();
    }

    // Local embedding providers have no remote endpoint to test.
    if entry.provider_type == "local" {
        return Json(TestProviderResponse {
            status: "ok".into(),
            message: "Local provider does not require a connection test".into(),
        })
        .into_response();
    }

    let endpoint = match &entry.endpoint {
        Some(ep) => ep.clone(),
        None => {
            return Json(TestProviderResponse {
                status: "error".into(),
                message: "endpoint is required for remote providers".into(),
            })
            .into_response();
        }
    };

    // Build a reqwest client with a 5-second timeout.
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Json(TestProviderResponse {
                status: "error".into(),
                message: format!("failed to build HTTP client: {e}"),
            })
            .into_response();
        }
    };

    // Send a minimal non-streaming chat completion request.
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": entry.model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
        "stream": false,
    });

    let mut request = client.post(&url).json(&body);
    if !api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }

    let result = request.send().await;

    match result {
        Ok(response) => {
            let status_code = response.status();
            if status_code.is_success() {
                Json(TestProviderResponse {
                    status: "ok".into(),
                    message: "Connected successfully".into(),
                })
                .into_response()
            } else {
                let body_text = response.text().await.unwrap_or_default();
                let error = crate::provider::openai::map_error_status(
                    status_code.as_u16(),
                    &body_text,
                );
                Json(TestProviderResponse {
                    status: "error".into(),
                    message: format!("{error}"),
                })
                .into_response()
            }
        }
        Err(e) => Json(TestProviderResponse {
            status: "error".into(),
            message: format!("Connection failed: {e}"),
        })
        .into_response(),
    }
}

// ── Memory management handlers ──────────────────────────────────────────

/// `POST /api/memory/migrate` — re-embed all stored memories using the current model.
pub async fn migrate_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let vs_snap = state.vector_store.load();
    let emb_snap = state.embedder.load();
    let vector_store = vs_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;
    let embedder = emb_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_embedder".into(),
                message: "no embedder configured".into(),
            }),
        )
    })?;

    // Read all existing entries.
    let entries = vector_store.list_all().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: format!("failed to list entries: {e}"),
            }),
        )
    })?;

    // Collect source texts for re-embedding.
    let texts: Vec<&str> = entries.iter().map(|e| e.source_text.as_str()).collect();

    let new_embeddings = if texts.is_empty() {
        Vec::new()
    } else {
        embedder.embed(&texts).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "embedding_failed".into(),
                    message: format!("re-embedding failed: {e}"),
                }),
            )
        })?
    };

    // Clear and re-store with new embeddings.
    vector_store.clear().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: format!("failed to clear store: {e}"),
            }),
        )
    })?;

    let count = entries.len();
    for (entry, embedding) in entries.into_iter().zip(new_embeddings) {
        let new_entry = crate::memory::VectorEntry {
            id: entry.id,
            embedding,
            source_text: entry.source_text,
            metadata: entry.metadata,
        };
        vector_store.store(new_entry).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "internal_error".into(),
                    message: format!("failed to store migrated entry: {e}"),
                }),
            )
        })?;
    }

    Ok(Json(serde_json::json!({ "migrated": count })))
}

/// `DELETE /api/memory` — clear all stored memories.
pub async fn clear_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let vs_snap = state.vector_store.load();
    let vector_store = vs_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;

    vector_store.clear().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "internal_error".into(),
                message: format!("failed to clear memory: {e}"),
            }),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

// ── Approval handler ─────────────────────────────────────────────────────

/// `POST /api/chat/{conversation_id}/approve` — approve or deny a pending skill execution.
pub async fn approve_handler<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(_conversation_id): Path<String>,
    Json(body): Json<ApproveRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let sender = {
        let mut pending = state.pending_approvals.lock().await;
        pending.remove(&body.approval_id)
    };

    match sender {
        Some(tx) => {
            let _ = tx.send(body.approved);
            Ok(StatusCode::OK)
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                code: "not_found".into(),
                message: format!("approval '{}' not found or already resolved", body.approval_id),
            }),
        )),
    }
}

// ── Chat handler ────────────────────────────────────────────────────────

/// `POST /api/chat` — accepts a `ChatRequest` and streams `ChatEvent` frames via SSE.
///
/// Implements the agentic tool-call loop: the LLM can request tool executions,
/// the backend runs them, feeds results back, and loops until a final text
/// response is produced or the iteration limit is reached.
///
/// If `conversation_id` is provided, loads history from that conversation.
/// If omitted/null, auto-creates a new conversation.
pub async fn chat_handler<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    body: Bytes,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ApiError>)> {
    let request: ChatRequest = serde_json::from_slice(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "bad_request".into(),
                message: format!("invalid request body: {e}"),
            }),
        )
    })?;

    // Resolve or create the conversation, loading existing messages when continuing.
    let (conversation_id, existing_messages) = match &request.conversation_id {
        Some(id) => {
            let conv = state.store.get_conversation(id).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        code: "internal_error".into(),
                        message: e,
                    }),
                )
            })?;
            match conv {
                Some(c) => (id.clone(), c.messages),
                None => {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ApiError {
                            code: "not_found".into(),
                            message: format!("conversation '{id}' not found"),
                        }),
                    ));
                }
            }
        }
        None => {
            // Auto-create a conversation, titled from the first user message.
            let title = request
                .messages
                .iter()
                .find(|m| matches!(m.content, MessageContent::Text { .. }) && matches!(m.role, Role::User))
                .and_then(|m| match &m.content {
                    MessageContent::Text { text } => Some(title_from_message(text)),
                    _ => None,
                })
                .unwrap_or_else(|| "New conversation".to_string());

            let conv = state.store.create_conversation(&title).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        code: "internal_error".into(),
                        message: e,
                    }),
                )
            })?;
            (conv.id, Vec::new())
        }
    };

    // Combine existing history with new messages for provider context.
    let new_messages = request.messages;
    let mut all_messages = existing_messages;
    let persist_from = all_messages.len();
    all_messages.extend(new_messages);

    let tools = {
        let registry = state.registry.load();
        let defs = registry.tool_definitions();
        if defs.is_empty() {
            None
        } else {
            Some(defs)
        }
    };

    // Channel for streaming events to the client.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<ChatEvent>(64);

    let conv_id = conversation_id.clone();
    let disable_memory = request.disable_memory;
    tokio::spawn(async move {
        run_tool_loop(state, conv_id, all_messages, persist_from, tools, tx, disable_memory).await;
    });

    let conv_id_for_meta = conversation_id;
    let events = async_stream::stream! {
        // Emit ConversationMeta as the first event.
        yield Ok::<_, Infallible>(
            Event::default().data(serde_json::to_string(&ChatEvent::ConversationMeta {
                conversation_id: conv_id_for_meta,
            }).unwrap())
        );

        while let Some(event) = rx.recv().await {
            let is_done = matches!(event, ChatEvent::Done);
            yield Ok::<_, Infallible>(
                Event::default().data(serde_json::to_string(&event).unwrap())
            );
            if is_done {
                break;
            }
        }
    };

    Ok(Sse::new(events))
}

/// Persist a message to the store, logging errors without crashing.
fn persist_message(state: &impl AsRef<crate::store::Store>, conversation_id: &str, message: &Message) {
    let store = state.as_ref();
    if let Err(e) = store.append_message(conversation_id, message) {
        eprintln!("warning: failed to persist message: {e}");
    }
}

/// Check whether a skill execution should proceed, applying the approval policy.
///
/// Returns `true` if the skill is approved (by policy, prior approval, or user action),
/// `false` if denied or timed out.
async fn check_approval<P: Provider>(
    state: &Arc<AppState<P>>,
    approval_overrides: &HashMap<String, ApprovalPolicy>,
    tx: &tokio::sync::mpsc::Sender<ChatEvent>,
    conversation_id: &str,
    skill_name: &str,
    arguments: &str,
    permission_level: PermissionLevel,
) -> bool {
    // Resolve effective policy: config override, or default Always for non-ReadOnly.
    let policy = approval_overrides
        .get(skill_name)
        .copied()
        .unwrap_or(ApprovalPolicy::Always);

    match policy {
        ApprovalPolicy::Trust => return true,
        ApprovalPolicy::Once => {
            let approvals = state.conversation_approvals.lock().await;
            if let Some(skills) = approvals.get(conversation_id) {
                if skills.contains(skill_name) {
                    return true;
                }
            }
        }
        ApprovalPolicy::Always => {}
    }

    // Need to ask the user.
    let approval_id = uuid::Uuid::new_v4().to_string();
    let (sender, receiver) = oneshot::channel::<bool>();

    {
        let mut pending = state.pending_approvals.lock().await;
        pending.insert(approval_id.clone(), sender);
    }

    let args_value: serde_json::Value = serde_json::from_str(arguments)
        .unwrap_or_else(|_| serde_json::json!({}));

    let perm_str = match permission_level {
        PermissionLevel::ReadOnly => "read_only",
        PermissionLevel::Mutating => "mutating",
        PermissionLevel::Network => "network",
    };

    let _ = tx
        .send(ChatEvent::ApprovalRequest {
            id: approval_id.clone(),
            skill_name: skill_name.to_string(),
            arguments: args_value,
            permission_level: perm_str.to_string(),
        })
        .await;

    let result = tokio::time::timeout(state.approval_timeout, receiver).await;

    // Cleanup pending entry regardless of outcome.
    {
        let mut pending = state.pending_approvals.lock().await;
        pending.remove(&approval_id);
    }

    match result {
        Ok(Ok(true)) => {
            // Record for `once` policy.
            if policy == ApprovalPolicy::Once {
                let mut approvals = state.conversation_approvals.lock().await;
                approvals
                    .entry(conversation_id.to_string())
                    .or_default()
                    .insert(skill_name.to_string());
            }
            true
        }
        _ => false,
    }
}

/// Run the tool-call loop, sending `ChatEvent`s through `tx`.
///
/// 1. Send messages + tool definitions to the provider.
/// 2. If the provider yields tool calls: execute them via the `SkillRegistry`,
///    append `ToolCall` and `ToolResult` messages, and call the provider again.
/// 3. Repeat until the provider returns only text (no tool calls).
/// 4. Text deltas are streamed to the client as `TokenDelta` events.
/// 5. Stops after `MAX_TOOL_ITERATIONS` to prevent runaway loops.
/// 6. All messages (user, assistant, tool calls, tool results) are persisted.
async fn run_tool_loop<P: Provider>(
    state: Arc<AppState<P>>,
    conversation_id: String,
    mut messages: Vec<Message>,
    persist_from: usize,
    tools: Option<Vec<serde_json::Value>>,
    tx: tokio::sync::mpsc::Sender<ChatEvent>,
    disable_memory: bool,
) {
    // Persist only new incoming messages (existing ones are already in the DB).
    for msg in &messages[persist_from..] {
        persist_message(&state.store, &conversation_id, msg);
    }

    // Emit current warnings at the start of the stream.
    let startup_warnings = {
        let collector = state.warnings.read().unwrap();
        collector.list().to_vec()
    };
    if !startup_warnings.is_empty() {
        let _ = tx.send(ChatEvent::Warnings { warnings: startup_warnings }).await;
    }

    // Load hot-reloadable state snapshots for the duration of this request.
    let memory_config = state.memory_config.load();
    let embedder = state.embedder.load();
    let vector_store = state.vector_store.load();
    let registry = state.registry.load();
    let provider = state.provider.load();
    let approval_overrides = state.approval_overrides.load();

    // Automatic context retrieval: search long-term memory for relevant memories.
    let mut recalled_context: Option<String> = None;
    if memory_config.auto_retrieve
        && !disable_memory
        && embedder.is_some()
        && vector_store.is_some()
    {
        // Find the latest user message text.
        let latest_user_text = messages
            .iter()
            .rev()
            .find_map(|m| match (&m.role, &m.content) {
                (Role::User, MessageContent::Text { text }) => Some(text.as_str()),
                _ => None,
            });

        if let Some(query_text) = latest_user_text {
            let emb = (**embedder).as_ref().unwrap();
            let vs = (**vector_store).as_ref().unwrap();

            if let Ok(embeddings) = emb.embed(&[query_text]) {
                if let Some(embedding) = embeddings.into_iter().next() {
                    if let Ok(results) = vs.search(&embedding, memory_config.auto_retrieve_limit) {
                        let threshold = memory_config.similarity_threshold;
                        let relevant: Vec<_> = results
                            .into_iter()
                            .filter(|r| r.score >= threshold)
                            .collect();

                        if !relevant.is_empty() {
                            // Build system prompt section.
                            let mut context_lines = vec!["## Recalled Memories".to_string()];
                            let mut snippets = Vec::new();
                            for r in &relevant {
                                let category = r.metadata.get("category")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());
                                let cat_label = category.as_deref().unwrap_or("general");
                                context_lines.push(format!(
                                    "- \"{}\" ({}, relevance: {:.2})",
                                    r.source_text, cat_label, r.score
                                ));
                                snippets.push(MemorySnippet {
                                    text: r.source_text.clone(),
                                    category,
                                    score: r.score,
                                });
                            }

                            recalled_context = Some(context_lines.join("\n"));
                            let _ = tx.send(ChatEvent::MemoryContext { memories: snippets }).await;
                        }
                    }
                }
            }
        }
    }

    for _iteration in 0..MAX_TOOL_ITERATIONS {
        // Inject recalled long-term memories and working memory as system context.
        let mut provider_messages = messages.clone();
        if let Some(ctx) = &recalled_context {
            provider_messages.insert(
                0,
                Message {
                    role: Role::System,
                    content: MessageContent::Text { text: ctx.clone() },
                    timestamp: Utc::now(),
                },
            );
        }
        {
            let wm_map = state.working_memory.lock().unwrap();
            if let Some(wm) = wm_map.get(&conversation_id) {
                if !wm.is_empty() {
                    provider_messages.insert(
                        0,
                        Message {
                            role: Role::System,
                            content: MessageContent::Text {
                                text: format!(
                                    "[Working Memory]\n{}",
                                    wm.to_context_string()
                                ),
                            },
                            timestamp: Utc::now(),
                        },
                    );
                }
            }
        }

        // Call the provider.
        let token_stream = match provider.complete(provider_messages, tools.clone()).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(ChatEvent::Error { message: e.to_string() }).await;
                let _ = tx.send(ChatEvent::Done).await;
                return;
            }
        };

        // Consume the stream, collecting text and tool calls.
        let mut tool_calls: Vec<(String, String, String)> = Vec::new();
        let mut full_text = String::new();

        tokio::pin!(token_stream);
        while let Some(result) = token_stream.next().await {
            match result {
                Ok(Token::Text { text }) => {
                    full_text.push_str(&text);
                    // Stream text deltas immediately.
                    let _ = tx
                        .send(ChatEvent::TokenDelta {
                            content: text,
                        })
                        .await;
                }
                Ok(Token::Warning { message }) => {
                    let _ = tx.send(ChatEvent::Warning { message }).await;
                }
                Ok(Token::ToolCall {
                    id,
                    name,
                    arguments,
                }) => {
                    tool_calls.push((id, name, arguments));
                }
                Err(e) => {
                    let _ = tx
                        .send(ChatEvent::Error {
                            message: e.to_string(),
                        })
                        .await;
                    let _ = tx.send(ChatEvent::Done).await;
                    return;
                }
            }
        }

        if tool_calls.is_empty() {
            // Final text response — persist and done.
            if !full_text.is_empty() {
                let assistant_msg = Message {
                    role: Role::Assistant,
                    content: MessageContent::Text { text: full_text },
                    timestamp: Utc::now(),
                };
                persist_message(&state.store, &conversation_id, &assistant_msg);
            }
            let _ = tx.send(ChatEvent::Done).await;
            return;
        }

        // Execute each tool call.
        for (id, name, arguments) in &tool_calls {
            // Notify the client that a tool call is starting.
            let _ = tx
                .send(ChatEvent::ToolCallStart {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                })
                .await;

            // Append the assistant's tool call to the conversation.
            let tool_call_msg = Message {
                role: Role::Assistant,
                content: MessageContent::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: arguments.clone(),
                },
                timestamp: Utc::now(),
            };
            persist_message(&state.store, &conversation_id, &tool_call_msg);
            messages.push(tool_call_msg);

            // Execute the skill (with approval check for non-ReadOnly skills).
            let result_content = match registry.get(name) {
                Some(skill) => {
                    let perm = skill.permission_level();
                    let approved = if perm == PermissionLevel::ReadOnly {
                        true
                    } else {
                        check_approval(
                            &state, &approval_overrides, &tx, &conversation_id, name, arguments, perm,
                        ).await
                    };

                    if !approved {
                        format!("User denied execution of {name}")
                    } else {
                        let mut input: serde_json::Value = serde_json::from_str(arguments)
                            .unwrap_or_else(|_| serde_json::json!({}));
                        // Inject conversation context so skills can access per-conversation state.
                        if let Some(obj) = input.as_object_mut() {
                            obj.insert(
                                "conversation_id".to_string(),
                                serde_json::Value::String(conversation_id.clone()),
                            );
                        }
                        match skill.execute(input).await {
                            Ok(output) => serde_json::to_string(&output)
                                .unwrap_or_else(|_| "{}".to_string()),
                            Err(e) => format!("Error: {e}"),
                        }
                    }
                }
                None => format!("Error: unknown tool '{name}'"),
            };

            // Notify the client of the result.
            let _ = tx
                .send(ChatEvent::ToolCallResult {
                    id: id.clone(),
                    content: result_content.clone(),
                })
                .await;

            // Append the tool result to the conversation.
            let tool_result_msg = Message {
                role: Role::User,
                content: MessageContent::ToolResult {
                    id: id.clone(),
                    content: result_content,
                },
                timestamp: Utc::now(),
            };
            persist_message(&state.store, &conversation_id, &tool_result_msg);
            messages.push(tool_result_msg);
        }

        // Loop: call the provider again with updated messages.
    }

    // Exceeded the maximum number of iterations.
    let _ = tx
        .send(ChatEvent::Error {
            message: format!(
                "Tool call loop exceeded maximum of {MAX_TOOL_ITERATIONS} iterations"
            ),
        })
        .await;
    let _ = tx.send(ChatEvent::Done).await;
}

impl AsRef<crate::store::Store> for crate::store::Store {
    fn as_ref(&self) -> &crate::store::Store {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use axum::Router;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use tower_http::services::ServeDir;

    use crate::provider::ProviderChain;
    use crate::skill::SkillRegistry;
    use crate::testutil::{
        ConfigurableMockProvider, FailingSkill, MockEchoSkill, MockProvider, MockResponse,
        SequencedProvider, make_chat_body, make_chat_body_with_conversation, post_chat,
        post_chat_raw,
    };

    // ── Helpers ─────────────────────────────────────────────────────────

    fn test_config() -> crate::config::Config {
        crate::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap()
    }

    fn registry_with_echo() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(MockEchoSkill));
        r
    }

    fn registry_with_failing() -> SkillRegistry {
        let mut r = SkillRegistry::new();
        r.register(Box::new(FailingSkill));
        r
    }

    fn test_app(tokens: Vec<String>) -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
    }

    fn test_app_with_static(tokens: Vec<String>, static_dir: &str) -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .with_state(state)
            .fallback_service(ServeDir::new(static_dir))
    }

    fn sequenced_app(responses: Vec<MockResponse>, registry: SkillRegistry) -> Router {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(SequencedProvider::new(responses)),
            registry: arc_swap::ArcSwap::from_pointee(registry),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        Router::new()
            .route("/api/chat", post(chat_handler::<SequencedProvider>))
            .with_state(state)
    }

    fn conversation_app(tokens: Vec<String>) -> (Arc<AppState<MockProvider>>, Router) {
        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
            registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
            store: crate::store::Store::open_in_memory().unwrap(),
            embedder: arc_swap::ArcSwap::from_pointee(None),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: std::time::Duration::from_secs(1),
            config: std::sync::RwLock::new(test_config()),
            config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
            on_config_change: None,
        });
        let router = Router::new()
            .route("/api/chat", post(chat_handler::<MockProvider>))
            .route(
                "/api/conversations",
                get(list_conversations::<MockProvider>).post(create_conversation::<MockProvider>),
            )
            .route(
                "/api/conversations/{id}",
                get(get_conversation::<MockProvider>).delete(delete_conversation::<MockProvider>),
            )
            .with_state(state.clone());
        (state, router)
    }

    // ── Chat tests ───────────────────────────────────────────────────

    mod chat {
        use super::*;

        #[tokio::test]
        async fn valid_request_streams_token_deltas_and_done() {
            let app = test_app(vec!["Hello".into(), " world".into()]);
            let events = post_chat(app, &make_chat_body()).await;

            assert_eq!(events.len(), 3);
            assert_eq!(
                events[0],
                ChatEvent::TokenDelta {
                    content: "Hello".into()
                }
            );
            assert_eq!(
                events[1],
                ChatEvent::TokenDelta {
                    content: " world".into()
                }
            );
            assert_eq!(events[2], ChatEvent::Done);
        }

        #[tokio::test]
        async fn malformed_json_returns_400_with_structured_error() {
            let app = test_app(vec![]);

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/chat")
                        .header("content-type", "application/json")
                        .body(Body::from("not valid json"))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);

            let body = response.into_body().collect().await.unwrap().to_bytes();
            let error: serde_json::Value = serde_json::from_slice(&body).unwrap();

            assert!(
                error.get("code").is_some(),
                "response should have 'code' field"
            );
            assert!(
                error.get("message").is_some(),
                "response should have 'message' field"
            );
            assert_eq!(error["code"], "bad_request");
        }

        #[tokio::test]
        async fn root_serves_index_html() {
            let dir = std::env::temp_dir().join("buddy-api-test-static");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("index.html"), "<html><body>buddy</body></html>").unwrap();

            let app = test_app_with_static(vec![], dir.to_str().unwrap());

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let ct = response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap();
            assert!(ct.contains("text/html"), "expected text/html, got {ct}");

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn nonexistent_asset_returns_404() {
            let dir = std::env::temp_dir().join("buddy-api-test-404");
            std::fs::create_dir_all(&dir).unwrap();

            let app = test_app_with_static(vec![], dir.to_str().unwrap());

            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/assets/nonexistent.js")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::NOT_FOUND);

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn normal_chat_no_tools_works_unchanged() {
            // With an empty registry (no tools), behavior is v0.1-style.
            let app = test_app(vec!["Hello!".into()]);
            let events = post_chat(app, &make_chat_body()).await;

            assert_eq!(events.len(), 2);
            assert_eq!(
                events[0],
                ChatEvent::TokenDelta {
                    content: "Hello!".into()
                }
            );
            assert_eq!(events[1], ChatEvent::Done);
        }
    }

    // ── Tool-call loop tests ────────────────────────────────────────────

    mod tool_loop {
        use super::*;

        #[tokio::test]
        async fn single_tool_call_executes_skill_and_returns_text() {
            let app = sequenced_app(
                vec![
                    // First call: LLM requests a tool call.
                    MockResponse::ToolCalls(vec![(
                        "call_1".into(),
                        "echo".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    // Second call: LLM returns text after seeing the tool result.
                    MockResponse::Text(vec!["The echo said hello.".into()]),
                ],
                registry_with_echo(),
            );

            let events = post_chat(app, &make_chat_body()).await;

            // Expect: ToolCallStart, ToolCallResult, TokenDelta, Done
            assert!(events.contains(&ChatEvent::ToolCallStart {
                id: "call_1".into(),
                name: "echo".into(),
                arguments: r#"{"value":"hello"}"#.into(),
            }));
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { id, content }
                if id == "call_1" && content.contains("hello")
            )));
            assert!(events.contains(&ChatEvent::TokenDelta {
                content: "The echo said hello.".into(),
            }));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        #[tokio::test]
        async fn three_chained_tool_calls_all_execute() {
            let app = sequenced_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "echo".into(),
                        r#"{"value":"a"}"#.into(),
                    )]),
                    MockResponse::ToolCalls(vec![(
                        "c2".into(),
                        "echo".into(),
                        r#"{"value":"b"}"#.into(),
                    )]),
                    MockResponse::ToolCalls(vec![(
                        "c3".into(),
                        "echo".into(),
                        r#"{"value":"c"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done chaining.".into()]),
                ],
                registry_with_echo(),
            );

            let events = post_chat(app, &make_chat_body()).await;

            let starts: Vec<_> = events
                .iter()
                .filter(|e| matches!(e, ChatEvent::ToolCallStart { .. }))
                .collect();
            let results: Vec<_> = events
                .iter()
                .filter(|e| matches!(e, ChatEvent::ToolCallResult { .. }))
                .collect();

            assert_eq!(starts.len(), 3);
            assert_eq!(results.len(), 3);
            assert!(events.contains(&ChatEvent::TokenDelta {
                content: "Done chaining.".into(),
            }));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        #[tokio::test]
        async fn loop_stops_at_max_iterations() {
            // 11 consecutive tool calls — should stop at 10.
            let mut responses: Vec<MockResponse> = (0..11)
                .map(|i| {
                    MockResponse::ToolCalls(vec![(
                        format!("c{i}"),
                        "echo".into(),
                        r#"{"value":"x"}"#.into(),
                    )])
                })
                .collect();
            // Unreachable final text.
            responses.push(MockResponse::Text(vec!["never reached".into()]));

            let app = sequenced_app(responses, registry_with_echo());
            let events = post_chat(app, &make_chat_body()).await;

            let starts: Vec<_> = events
                .iter()
                .filter(|e| matches!(e, ChatEvent::ToolCallStart { .. }))
                .collect();

            // Should execute exactly MAX_TOOL_ITERATIONS tool calls.
            assert_eq!(starts.len(), MAX_TOOL_ITERATIONS);

            // Should have an error about exceeding the limit.
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::Error { message } if message.contains("exceeded")
            )));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        #[tokio::test]
        async fn skill_error_is_fed_back_not_crash() {
            let app = sequenced_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "failing".into(),
                        "{}".into(),
                    )]),
                    MockResponse::Text(vec!["Handled the error.".into()]),
                ],
                registry_with_failing(),
            );

            let events = post_chat(app, &make_chat_body()).await;

            // The tool result should contain the error message.
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("Error:")
            )));

            // The conversation should continue — no fatal crash.
            assert!(events.contains(&ChatEvent::TokenDelta {
                content: "Handled the error.".into(),
            }));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        #[tokio::test]
        async fn unknown_tool_returns_error_result() {
            let app = sequenced_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "nonexistent".into(),
                        "{}".into(),
                    )]),
                    MockResponse::Text(vec!["OK.".into()]),
                ],
                SkillRegistry::new(), // empty registry
            );

            let events = post_chat(app, &make_chat_body()).await;

            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("unknown tool")
            )));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        #[tokio::test]
        async fn sse_stream_contains_tool_events() {
            let app = sequenced_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "tc1".into(),
                        "echo".into(),
                        r#"{"value":"test"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Final.".into()]),
                ],
                registry_with_echo(),
            );

            let events = post_chat(app, &make_chat_body()).await;

            let has_start = events.iter().any(|e| matches!(e, ChatEvent::ToolCallStart { .. }));
            let has_result = events.iter().any(|e| matches!(e, ChatEvent::ToolCallResult { .. }));

            assert!(has_start, "expected ToolCallStart in SSE stream");
            assert!(has_result, "expected ToolCallResult in SSE stream");
        }

        #[tokio::test]
        async fn fallback_emits_warning_event_in_sse_stream() {
            let chain = ProviderChain::new(vec![
                (
                    ConfigurableMockProvider::FailNetwork("down".into()),
                    "primary".into(),
                ),
                (
                    ConfigurableMockProvider::Succeed(vec!["fallback response".into()]),
                    "fallback-model".into(),
                ),
            ]);
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(chain),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: crate::warning::new_shared_warnings(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            let app = Router::new()
                .route(
                    "/api/chat",
                    post(chat_handler::<ProviderChain<ConfigurableMockProvider>>),
                )
                .with_state(state);

            let events = post_chat(app, &make_chat_body()).await;

            // Warning should appear before the token delta.
            let warning_idx = events
                .iter()
                .position(|e| matches!(e, ChatEvent::Warning { .. }))
                .expect("expected a Warning event in the SSE stream");
            let delta_idx = events
                .iter()
                .position(|e| matches!(e, ChatEvent::TokenDelta { .. }))
                .expect("expected a TokenDelta event");
            assert!(
                warning_idx < delta_idx,
                "Warning should come before TokenDelta"
            );

            // Verify the warning mentions the fallback model.
            assert!(matches!(
                &events[warning_idx],
                ChatEvent::Warning { message } if message.contains("fallback-model")
            ));
            assert!(events.last() == Some(&ChatEvent::Done));
        }
    }

    // ── Conversation management tests ──────────────────────────────────

    mod conversations {
        use super::*;

        #[tokio::test]
        async fn list_conversations_empty_on_fresh_db() {
            let (_, app) = conversation_app(vec![]);
            let response = app
                .oneshot(Request::builder().uri("/api/conversations").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
            assert!(list.is_empty());
        }

        #[tokio::test]
        async fn create_then_list_conversation() {
            let (_, app) = conversation_app(vec![]);

            // Create
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/conversations")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let conv: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(conv.get("id").is_some());

            // List
            let response = app
                .oneshot(Request::builder().uri("/api/conversations").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0]["id"], conv["id"]);
        }

        #[tokio::test]
        async fn get_nonexistent_conversation_returns_404() {
            let (_, app) = conversation_app(vec![]);
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/conversations/nonexistent-id")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(err["code"], "not_found");
        }

        #[tokio::test]
        async fn delete_conversation_returns_204() {
            let (state, app) = conversation_app(vec![]);
            let conv = state.store.create_conversation("To delete").unwrap();

            let response = app
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri(format!("/api/conversations/{}", conv.id))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);

            // Verify it's gone.
            assert!(state.store.get_conversation(&conv.id).unwrap().is_none());
        }

        #[tokio::test]
        async fn delete_nonexistent_conversation_returns_404() {
            let (_, app) = conversation_app(vec![]);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("DELETE")
                        .uri("/api/conversations/nonexistent-id")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn chat_without_conversation_id_auto_creates() {
            let (state, app) = conversation_app(vec!["Reply".into()]);

            let events = post_chat_raw(app, &make_chat_body()).await;

            // First event should be ConversationMeta.
            assert!(
                matches!(&events[0], ChatEvent::ConversationMeta { conversation_id } if !conversation_id.is_empty()),
                "first event should be ConversationMeta"
            );

            // A conversation should have been auto-created.
            let convs = state.store.list_conversations().unwrap();
            assert_eq!(convs.len(), 1);
            assert_eq!(convs[0].title, "Hi"); // title from the user message
        }

        #[tokio::test]
        async fn chat_with_conversation_id_appends_to_existing() {
            let (state, app) = conversation_app(vec!["Reply".into()]);
            let conv = state.store.create_conversation("Existing").unwrap();

            let body = make_chat_body_with_conversation(&conv.id);
            let events = post_chat_raw(app, &body).await;

            // Should get ConversationMeta with the provided id.
            assert!(matches!(
                &events[0],
                ChatEvent::ConversationMeta { conversation_id } if conversation_id == &conv.id
            ));

            // Messages should be persisted.
            let loaded = state.store.get_conversation(&conv.id).unwrap().unwrap();
            assert!(loaded.messages.len() >= 2); // user + assistant
        }

        #[tokio::test]
        async fn chat_with_nonexistent_conversation_id_returns_404() {
            let (_, app) = conversation_app(vec![]);
            let body = make_chat_body_with_conversation("nonexistent-id");

            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/chat")
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        #[tokio::test]
        async fn chat_persists_all_message_types() {
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(SequencedProvider::new(vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "echo".into(),
                        r#"{"value":"test"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Final answer.".into()]),
                ])),
                registry: arc_swap::ArcSwap::from_pointee(registry_with_echo()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: crate::warning::new_shared_warnings(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            let app = Router::new()
                .route("/api/chat", post(chat_handler::<SequencedProvider>))
                .route(
                    "/api/conversations",
                    get(list_conversations::<SequencedProvider>).post(create_conversation::<SequencedProvider>),
                )
                .route(
                    "/api/conversations/{id}",
                    get(get_conversation::<SequencedProvider>).delete(delete_conversation::<SequencedProvider>),
                )
                .with_state(state.clone());

            let events = post_chat_raw(app, &make_chat_body()).await;

            // Get the conversation id from the meta event.
            let conv_id = match &events[0] {
                ChatEvent::ConversationMeta { conversation_id } => conversation_id.clone(),
                _ => panic!("expected ConversationMeta as first event"),
            };

            let conv = state.store.get_conversation(&conv_id).unwrap().unwrap();

            // Should have: user msg, tool call, tool result, assistant text = 4 messages
            assert_eq!(conv.messages.len(), 4, "expected 4 persisted messages, got {}: {:?}", conv.messages.len(), conv.messages);

            // Verify types
            assert!(matches!(conv.messages[0].content, MessageContent::Text { .. }));
            assert!(matches!(conv.messages[1].content, MessageContent::ToolCall { .. }));
            assert!(matches!(conv.messages[2].content, MessageContent::ToolResult { .. }));
            assert!(matches!(conv.messages[3].content, MessageContent::Text { .. }));
        }

        #[tokio::test]
        async fn sse_stream_starts_with_conversation_meta() {
            let (_, app) = conversation_app(vec!["Hi".into()]);
            let events = post_chat_raw(app, &make_chat_body()).await;

            assert!(!events.is_empty());
            assert!(
                matches!(&events[0], ChatEvent::ConversationMeta { conversation_id } if !conversation_id.is_empty()),
                "SSE stream must start with ConversationMeta"
            );
        }
    }

    // ── Warning system tests ──────────────────────────────────────────────

    mod warnings {
        use super::*;
        use crate::warning::{new_shared_warnings, Warning, WarningSeverity};

        fn warnings_app(
            tokens: Vec<String>,
            setup: impl FnOnce(&mut crate::warning::WarningCollector),
        ) -> Router {
            let warnings = new_shared_warnings();
            {
                let mut collector = warnings.write().unwrap();
                setup(&mut collector);
            }
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings,
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            Router::new()
                .route("/api/chat", post(chat_handler::<MockProvider>))
                .route("/api/warnings", get(get_warnings::<MockProvider>))
                .with_state(state)
        }

        #[tokio::test]
        async fn no_embedding_warning_present() {
            let app = warnings_app(vec![], |c| {
                c.add(Warning {
                    code: "no_embedding_model".into(),
                    message: "No embedding model configured — memory features are disabled. Add a [models.embedding] section to buddy.toml.".into(),
                    severity: WarningSeverity::Warning,
                });
            });

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].code, "no_embedding_model");
        }

        #[tokio::test]
        async fn full_config_no_warnings() {
            let app = warnings_app(vec![], |_| {});

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert!(list.is_empty());
        }

        #[tokio::test]
        async fn single_chat_provider_info() {
            let app = warnings_app(vec![], |c| {
                c.add(Warning {
                    code: "single_chat_provider".into(),
                    message: "Only one chat provider configured — no fallback available. Add additional [[models.chat.providers]] entries to buddy.toml for redundancy.".into(),
                    severity: WarningSeverity::Info,
                });
            });

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].code, "single_chat_provider");
            assert_eq!(list[0].severity, WarningSeverity::Info);
        }

        #[tokio::test]
        async fn runtime_warning_appears() {
            let warnings = new_shared_warnings();
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec![] }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: warnings.clone(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            let app = Router::new()
                .route("/api/warnings", get(get_warnings::<MockProvider>))
                .with_state(state);

            // Add a warning at runtime.
            {
                let mut collector = warnings.write().unwrap();
                collector.add(Warning {
                    code: "runtime_issue".into(),
                    message: "Something went wrong at runtime.".into(),
                    severity: WarningSeverity::Warning,
                });
            }

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].code, "runtime_issue");
        }

        #[tokio::test]
        async fn clear_warning_removes_it() {
            let warnings = new_shared_warnings();
            {
                let mut collector = warnings.write().unwrap();
                collector.add(Warning {
                    code: "to_clear".into(),
                    message: "Will be cleared.".into(),
                    severity: WarningSeverity::Warning,
                });
                collector.add(Warning {
                    code: "keep_me".into(),
                    message: "Should remain.".into(),
                    severity: WarningSeverity::Info,
                });
            }
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec![] }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: warnings.clone(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            let app = Router::new()
                .route("/api/warnings", get(get_warnings::<MockProvider>))
                .with_state(state);

            // Clear one warning.
            {
                let mut collector = warnings.write().unwrap();
                collector.clear("to_clear");
            }

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].code, "keep_me");
        }

        #[tokio::test]
        async fn sse_stream_includes_warnings_event() {
            let app = warnings_app(vec!["Hello".into()], |c| {
                c.add(Warning {
                    code: "test_warning".into(),
                    message: "A test warning.".into(),
                    severity: WarningSeverity::Warning,
                });
            });

            let events = post_chat_raw(app, &make_chat_body()).await;

            // ConversationMeta should be first, then Warnings before TokenDelta.
            assert!(matches!(&events[0], ChatEvent::ConversationMeta { .. }));
            let warnings_idx = events
                .iter()
                .position(|e| matches!(e, ChatEvent::Warnings { .. }))
                .expect("expected a Warnings event in the SSE stream");
            let delta_idx = events
                .iter()
                .position(|e| matches!(e, ChatEvent::TokenDelta { .. }))
                .expect("expected a TokenDelta event");
            assert!(
                warnings_idx < delta_idx,
                "Warnings should come before TokenDelta"
            );

            // Verify the warning content.
            if let ChatEvent::Warnings { warnings } = &events[warnings_idx] {
                assert_eq!(warnings.len(), 1);
                assert_eq!(warnings[0].code, "test_warning");
            } else {
                panic!("expected Warnings event");
            }
        }

        #[tokio::test]
        async fn warning_messages_include_guidance() {
            let app = warnings_app(vec![], |c| {
                c.add(Warning {
                    code: "no_embedding_model".into(),
                    message: "No embedding model configured — memory features are disabled. Add a [models.embedding] section to buddy.toml.".into(),
                    severity: WarningSeverity::Warning,
                });
            });

            let response = app
                .oneshot(Request::builder().uri("/api/warnings").body(Body::empty()).unwrap())
                .await
                .unwrap();
            let body = response.into_body().collect().await.unwrap().to_bytes();
            let list: Vec<Warning> = serde_json::from_slice(&body).unwrap();
            assert!(
                list[0].message.contains("buddy.toml"),
                "warning message should include guidance referencing buddy.toml: {}",
                list[0].message
            );
        }
    }

    // ── Approval tests ─────────────────────────────────────────────────

    mod approval {
        use super::*;
        use crate::config::ApprovalPolicy;
        use crate::testutil::{MockMutatingSkill, MockNetworkSkill};

        fn registry_with_mutating() -> SkillRegistry {
            let mut r = SkillRegistry::new();
            r.register(Box::new(MockMutatingSkill));
            r
        }

        fn registry_with_network() -> SkillRegistry {
            let mut r = SkillRegistry::new();
            r.register(Box::new(MockNetworkSkill));
            r
        }

        fn approval_app(
            responses: Vec<MockResponse>,
            registry: SkillRegistry,
            overrides: HashMap<String, ApprovalPolicy>,
            timeout: std::time::Duration,
        ) -> (Arc<AppState<SequencedProvider>>, Router) {
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(SequencedProvider::new(responses)),
                registry: arc_swap::ArcSwap::from_pointee(registry),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: crate::warning::new_shared_warnings(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(overrides),
                approval_timeout: timeout,
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            let router = Router::new()
                .route("/api/chat", post(chat_handler::<SequencedProvider>))
                .with_state(state.clone());
            (state, router)
        }

        // 1. ReadOnly executes without approval
        #[tokio::test]
        async fn readonly_executes_without_approval() {
            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "echo".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_echo(),
                HashMap::new(),
                std::time::Duration::from_secs(1),
            );

            let events = post_chat(app, &make_chat_body()).await;

            // No ApprovalRequest in the stream.
            assert!(
                !events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
                "ReadOnly skill should not emit ApprovalRequest"
            );
            // Skill executed successfully.
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("hello")
            )));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        // 2. Mutating emits ApprovalRequest — timeout → denied
        #[tokio::test]
        async fn mutating_emits_approval_request_and_times_out() {
            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                HashMap::new(),
                std::time::Duration::from_millis(50),
            );

            let events = post_chat(app, &make_chat_body()).await;

            // ApprovalRequest should be in the stream.
            assert!(
                events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
                "Mutating skill should emit ApprovalRequest"
            );
            // Should be denied (timeout).
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("User denied execution of mutating")
            )));
        }

        // 3. Approve mutating skill
        #[tokio::test]
        async fn approve_mutating_skill_executes() {
            let (state, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                HashMap::new(),
                std::time::Duration::from_secs(5),
            );

            // Background task that auto-approves.
            let pending = state.pending_approvals.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    let mut map = pending.lock().await;
                    let keys: Vec<String> = map.keys().cloned().collect();
                    for key in keys {
                        if let Some(tx) = map.remove(&key) {
                            let _ = tx.send(true);
                        }
                    }
                }
            });

            let events = post_chat(app, &make_chat_body()).await;

            // Skill should have executed (echo result).
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("hello")
            )));
            assert!(events.last() == Some(&ChatEvent::Done));
        }

        // 4. Deny mutating skill
        #[tokio::test]
        async fn deny_mutating_skill_returns_denied() {
            let (state, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                HashMap::new(),
                std::time::Duration::from_secs(5),
            );

            // Background task that denies.
            let pending = state.pending_approvals.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    let mut map = pending.lock().await;
                    let keys: Vec<String> = map.keys().cloned().collect();
                    for key in keys {
                        if let Some(tx) = map.remove(&key) {
                            let _ = tx.send(false);
                        }
                    }
                }
            });

            let events = post_chat(app, &make_chat_body()).await;

            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("User denied execution of mutating")
            )));
        }

        // 5. Trust policy auto-approves
        #[tokio::test]
        async fn trust_policy_auto_approves() {
            let mut overrides = HashMap::new();
            overrides.insert("mutating".into(), ApprovalPolicy::Trust);

            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                overrides,
                std::time::Duration::from_secs(1),
            );

            let events = post_chat(app, &make_chat_body()).await;

            // No ApprovalRequest.
            assert!(
                !events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
                "Trust policy should not emit ApprovalRequest"
            );
            // Skill executed.
            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("hello")
            )));
        }

        // 6. Once policy — first requires approval, second auto-approves
        #[tokio::test]
        async fn once_policy_asks_first_then_auto_approves() {
            let mut overrides = HashMap::new();
            overrides.insert("mutating".into(), ApprovalPolicy::Once);

            let (state, app) = approval_app(
                vec![
                    // First tool call.
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"first"}"#.into(),
                    )]),
                    // Second tool call.
                    MockResponse::ToolCalls(vec![(
                        "c2".into(),
                        "mutating".into(),
                        r#"{"value":"second"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                overrides,
                std::time::Duration::from_secs(5),
            );

            // Auto-approve the first request.
            let pending = state.pending_approvals.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    let mut map = pending.lock().await;
                    let keys: Vec<String> = map.keys().cloned().collect();
                    for key in keys {
                        if let Some(tx) = map.remove(&key) {
                            let _ = tx.send(true);
                        }
                    }
                }
            });

            let events = post_chat(app, &make_chat_body()).await;

            // Should have exactly one ApprovalRequest (for the first call).
            let approval_count = events
                .iter()
                .filter(|e| matches!(e, ChatEvent::ApprovalRequest { .. }))
                .count();
            assert_eq!(
                approval_count, 1,
                "Once policy should emit ApprovalRequest only on first call"
            );

            // Both calls should have executed.
            let result_count = events
                .iter()
                .filter(|e| matches!(e, ChatEvent::ToolCallResult { content, .. } if content.contains("echo")))
                .count();
            assert_eq!(result_count, 2, "Both tool calls should have executed");
        }

        // 7. Network skill emits ApprovalRequest
        #[tokio::test]
        async fn network_skill_emits_approval_request() {
            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "network".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_network(),
                HashMap::new(),
                std::time::Duration::from_millis(50),
            );

            let events = post_chat(app, &make_chat_body()).await;

            assert!(
                events.iter().any(|e| matches!(e, ChatEvent::ApprovalRequest { .. })),
                "Network skill should emit ApprovalRequest"
            );
        }

        // 8. Timeout treated as denied
        #[tokio::test]
        async fn timeout_treated_as_denied() {
            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                HashMap::new(),
                std::time::Duration::from_millis(50),
            );

            let events = post_chat(app, &make_chat_body()).await;

            assert!(events.iter().any(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("User denied")
            )));
        }

        // 9. ApprovalRequest event shape
        #[tokio::test]
        async fn approval_request_event_shape() {
            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                HashMap::new(),
                std::time::Duration::from_millis(50),
            );

            let events = post_chat(app, &make_chat_body()).await;

            let approval = events.iter().find(|e| matches!(e, ChatEvent::ApprovalRequest { .. }));
            assert!(approval.is_some(), "should contain ApprovalRequest");

            if let Some(ChatEvent::ApprovalRequest { id, skill_name, arguments, permission_level }) = approval {
                assert!(!id.is_empty(), "approval id should not be empty");
                assert_eq!(skill_name, "mutating");
                assert_eq!(arguments["value"], "hello");
                assert_eq!(permission_level, "mutating");
            }
        }

        // 10. Denied message is informative
        #[tokio::test]
        async fn denied_message_is_informative() {
            let (_, app) = approval_app(
                vec![
                    MockResponse::ToolCalls(vec![(
                        "c1".into(),
                        "mutating".into(),
                        r#"{"value":"hello"}"#.into(),
                    )]),
                    MockResponse::Text(vec!["Done.".into()]),
                ],
                registry_with_mutating(),
                HashMap::new(),
                std::time::Duration::from_millis(50),
            );

            let events = post_chat(app, &make_chat_body()).await;

            let denied = events.iter().find(|e| matches!(
                e,
                ChatEvent::ToolCallResult { content, .. } if content.contains("denied")
            ));
            assert!(denied.is_some(), "should contain denied tool result");

            if let Some(ChatEvent::ToolCallResult { content, .. }) = denied {
                assert!(
                    content.contains("User denied execution of"),
                    "denied message should be informative: {content}"
                );
            }
        }
    }

    mod config_api {
        use super::*;

        fn config_app(config: crate::config::Config) -> Router {
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                    tokens: vec!["hi".into()],
                }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: crate::warning::new_shared_warnings(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(config),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            Router::new()
                .route("/api/config", get(get_config::<MockProvider>))
                .with_state(state)
        }

        #[tokio::test]
        async fn full_config_returns_all_sections() {
            let config = crate::config::Config::parse(
                r#"
[server]
host = "0.0.0.0"
port = 8080

[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"

[chat]
system_prompt = "Be helpful."

[skills.read_file]
allowed_directories = ["/tmp"]

[skills.write_file]
allowed_directories = ["/tmp"]

[skills.fetch_url]
allowed_domains = ["example.com"]

[memory]
auto_retrieve = false
auto_retrieve_limit = 5
similarity_threshold = 0.8
"#,
            )
            .unwrap();

            let app = config_app(config);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value =
                serde_json::from_slice(&bytes).expect("response should be valid JSON");

            // Models section
            let chat_providers = &json["models"]["chat"]["providers"];
            assert_eq!(chat_providers.as_array().unwrap().len(), 2);
            assert_eq!(chat_providers[0]["type"], "openai");
            assert_eq!(chat_providers[0]["model"], "gpt-4");
            assert_eq!(chat_providers[1]["type"], "lmstudio");

            let emb_providers = &json["models"]["embedding"]["providers"];
            assert_eq!(emb_providers.as_array().unwrap().len(), 1);
            assert_eq!(emb_providers[0]["type"], "local");

            // Chat section
            assert_eq!(json["chat"]["system_prompt"], "Be helpful.");

            // Server section
            assert_eq!(json["server"]["host"], "0.0.0.0");
            assert_eq!(json["server"]["port"], 8080);

            // Skills section
            assert!(json["skills"]["read_file"].is_object());
            assert!(json["skills"]["write_file"].is_object());
            assert!(json["skills"]["fetch_url"].is_object());

            // Memory section
            assert_eq!(json["memory"]["auto_retrieve"], false);
            assert_eq!(json["memory"]["auto_retrieve_limit"], 5);
        }

        #[tokio::test]
        async fn minimal_config_returns_nulls_for_optional_sections() {
            let config = crate::config::Config::parse(
                r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
            )
            .unwrap();

            let app = config_app(config);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

            assert!(json["models"]["embedding"].is_null());
            assert!(json["skills"]["read_file"].is_null());
            assert!(json["skills"]["write_file"].is_null());
            assert!(json["skills"]["fetch_url"].is_null());
        }

        #[tokio::test]
        async fn api_key_env_present_but_secret_not_leaked() {
            let config = crate::config::Config::parse(
                r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "BUDDY_TEST_SECRET_029"
"#,
            )
            .unwrap();

            let app = config_app(config);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let body = String::from_utf8(bytes.to_vec()).unwrap();

            // The env var name should appear in the response.
            assert!(
                body.contains("BUDDY_TEST_SECRET_029"),
                "api_key_env name should be present"
            );

            // Set the env var to a known value and verify it does NOT appear.
            unsafe { std::env::set_var("BUDDY_TEST_SECRET_029", "super-secret-key-value") };
            assert!(
                !body.contains("super-secret-key-value"),
                "resolved secret must not appear in the response"
            );
            unsafe { std::env::remove_var("BUDDY_TEST_SECRET_029") };
        }

        #[tokio::test]
        async fn round_trip_json_to_config() {
            let config = crate::config::Config::parse(
                r#"
[server]
host = "127.0.0.1"
port = 3000

[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "MY_KEY"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"

[chat]
system_prompt = "Hello"

[skills.read_file]
allowed_directories = ["/tmp"]

[memory]
auto_retrieve = true
auto_retrieve_limit = 3
similarity_threshold = 0.5
"#,
            )
            .unwrap();

            let app = config_app(config);
            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let deserialized: crate::config::Config =
                serde_json::from_slice(&bytes).expect("should deserialize back into Config");

            assert_eq!(deserialized.server.host, "127.0.0.1");
            assert_eq!(deserialized.server.port, 3000);
            assert_eq!(deserialized.models.chat.providers.len(), 1);
            assert_eq!(deserialized.models.chat.providers[0].model, "gpt-4");
            assert!(deserialized.models.embedding.is_some());
            assert_eq!(deserialized.chat.system_prompt, "Hello");
            assert!(deserialized.skills.read_file.is_some());
            assert!(deserialized.memory.auto_retrieve);
        }

        /// Helper that creates a temp dir with a real config file and returns (temp_dir, Router).
        fn config_write_app() -> (std::path::PathBuf, Router) {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("buddy-config-write-{}-{}", std::process::id(), id));
            std::fs::create_dir_all(&dir).unwrap();
            let config_path = dir.join("buddy.toml");
            let initial_toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#;
            std::fs::write(&config_path, initial_toml).unwrap();
            let config = crate::config::Config::parse(initial_toml).unwrap();
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider { tokens: vec!["hi".into()] }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(crate::config::MemoryConfig::default()),
                warnings: crate::warning::new_shared_warnings(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(config),
                config_path,
                on_config_change: None,
            });
            let router = Router::new()
                .route("/api/config", get(get_config::<MockProvider>))
                .route("/api/config/models", axum::routing::put(put_config_models::<MockProvider>))
                .route("/api/config/skills", axum::routing::put(put_config_skills::<MockProvider>))
                .route("/api/config/chat", axum::routing::put(put_config_chat::<MockProvider>))
                .route("/api/config/server", axum::routing::put(put_config_server::<MockProvider>))
                .route("/api/config/memory", axum::routing::put(put_config_memory::<MockProvider>))
                .with_state(state);
            (dir, router)
        }

        #[tokio::test]
        async fn put_valid_models_persists_to_disk() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "chat": {
                    "providers": [{
                        "type": "openai",
                        "model": "gpt-4o",
                        "endpoint": "https://api.openai.com/v1",
                        "api_key_env": "MY_KEY"
                    }]
                }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
            let reparsed = crate::config::Config::parse(&disk).unwrap();
            assert_eq!(reparsed.models.chat.providers[0].model, "gpt-4o");
            assert_eq!(reparsed.models.chat.providers[0].provider_type, "openai");

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_models_empty_providers_returns_400() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "chat": { "providers": [] }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let err: ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
            assert!(err.errors.iter().any(|e| e.field.contains("models.chat.providers")));

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_models_unknown_provider_type_returns_400() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "chat": {
                    "providers": [{
                        "type": "anthropic",
                        "model": "claude"
                    }]
                }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let err: ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
            assert!(err.errors.iter().any(|e| e.field.contains("type")));

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_models_empty_model_string_returns_400() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "chat": {
                    "providers": [{
                        "type": "openai",
                        "model": ""
                    }]
                }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let err: ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
            assert!(err.errors.iter().any(|e| e.field.contains("model")));

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_skills_updates_skills_preserves_others() {
            let (dir, app) = config_write_app();
            let tmp = std::env::temp_dir();
            let body = serde_json::json!({
                "read_file": {
                    "allowed_directories": [tmp.to_str().unwrap()]
                }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/skills")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
            let reparsed = crate::config::Config::parse(&disk).unwrap();
            assert!(reparsed.skills.read_file.is_some());
            // Models section should be unchanged.
            assert_eq!(reparsed.models.chat.providers[0].model, "test-model");

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_chat_persists_system_prompt() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "system_prompt": "You are a pirate."
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/chat")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
            let reparsed = crate::config::Config::parse(&disk).unwrap();
            assert_eq!(reparsed.chat.system_prompt, "You are a pirate.");

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_server_persists_port() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "host": "0.0.0.0",
                "port": 8080
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/server")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
            let reparsed = crate::config::Config::parse(&disk).unwrap();
            assert_eq!(reparsed.server.port, 8080);
            assert_eq!(reparsed.server.host, "0.0.0.0");

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn put_memory_persists() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "auto_retrieve": false,
                "auto_retrieve_limit": 10,
                "similarity_threshold": 0.9
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/memory")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let disk = std::fs::read_to_string(dir.join("buddy.toml")).unwrap();
            let reparsed = crate::config::Config::parse(&disk).unwrap();
            assert!(!reparsed.memory.auto_retrieve);
            assert_eq!(reparsed.memory.auto_retrieve_limit, 10);

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn get_config_reflects_put_change() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "system_prompt": "Changed prompt."
            });
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/chat")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["chat"]["system_prompt"], "Changed prompt.");

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn validation_returns_all_failures() {
            let (dir, app) = config_write_app();
            let body = serde_json::json!({
                "chat": {
                    "providers": [
                        { "type": "unknown1", "model": "" },
                        { "type": "unknown2", "model": "" }
                    ]
                }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let err: ValidationErrorResponse = serde_json::from_slice(&bytes).unwrap();
            // Should have 4 errors: 2 unknown types + 2 empty models
            assert_eq!(err.errors.len(), 4, "expected 4 validation errors, got: {:?}", err.errors);

            std::fs::remove_dir_all(&dir).ok();
        }

        // ── test-provider endpoint tests ────────────────────────────────

        fn test_provider_app() -> Router {
            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                    tokens: vec!["hi".into()],
                }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(
                    crate::config::MemoryConfig::default(),
                ),
                warnings: crate::warning::new_shared_warnings(),
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(test_config()),
                config_path: std::path::PathBuf::from("/tmp/buddy-test.toml"),
                on_config_change: None,
            });
            Router::new()
                .route(
                    "/api/config/test-provider",
                    post(test_provider::<MockProvider>),
                )
                .route("/api/config", get(get_config::<MockProvider>))
                .with_state(state)
        }

        #[tokio::test]
        async fn test_provider_unknown_type_returns_400() {
            let app = test_provider_app();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/test-provider")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "type": "unknown",
                                "model": "some-model",
                                "endpoint": "http://localhost:1234/v1"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(json["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains("unknown provider type"));
        }

        #[tokio::test]
        async fn test_provider_empty_model_returns_400() {
            let app = test_provider_app();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/test-provider")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "type": "openai",
                                "model": "",
                                "endpoint": "http://localhost:1234/v1"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert!(json["errors"][0]["message"]
                .as_str()
                .unwrap()
                .contains("must not be empty"));
        }

        #[tokio::test]
        async fn test_provider_missing_env_var_returns_error() {
            let app = test_provider_app();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/test-provider")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "type": "openai",
                                "model": "gpt-4",
                                "endpoint": "https://api.openai.com/v1",
                                "api_key_env": "BUDDY_TEST_NOTSET_032"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["status"], "error");
            assert!(json["message"]
                .as_str()
                .unwrap()
                .contains("BUDDY_TEST_NOTSET_032"));
        }

        #[tokio::test]
        async fn test_provider_unreachable_endpoint_returns_error() {
            let app = test_provider_app();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/test-provider")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "type": "lmstudio",
                                "model": "test-model",
                                "endpoint": "http://127.0.0.1:1"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["status"], "error");
            assert!(json["message"]
                .as_str()
                .unwrap()
                .contains("Connection failed"));
        }

        #[tokio::test]
        async fn test_provider_does_not_modify_config() {
            let app = test_provider_app();

            // Read config before.
            let before = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let before_bytes = before.into_body().collect().await.unwrap().to_bytes();

            // Fire test-provider (will fail — unreachable endpoint).
            let _ = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/test-provider")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "type": "lmstudio",
                                "model": "test-model",
                                "endpoint": "http://127.0.0.1:1"
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();

            // Read config after.
            let after = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/config")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let after_bytes = after.into_body().collect().await.unwrap().to_bytes();

            assert_eq!(before_bytes, after_bytes);
        }

        #[tokio::test]
        async fn test_provider_timeout_when_endpoint_hangs() {
            // Start a TCP listener that accepts but never responds.
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            // Keep the listener alive but never read/write.
            let _handle = tokio::spawn(async move {
                loop {
                    let _ = listener.accept().await;
                }
            });

            let app = test_provider_app();
            let start = std::time::Instant::now();
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/config/test-provider")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            serde_json::json!({
                                "type": "lmstudio",
                                "model": "test-model",
                                "endpoint": format!("http://{addr}")
                            })
                            .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            let elapsed = start.elapsed();

            assert_eq!(response.status(), StatusCode::OK);
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["status"], "error");

            // Should complete within ~5s (with slack), not hang forever.
            assert!(
                elapsed < std::time::Duration::from_secs(10),
                "request took too long: {elapsed:?}"
            );
        }
    }

    mod hot_reload {
        use super::*;
        use axum::routing::put;
        use crate::warning::{new_shared_warnings, Warning, WarningSeverity};

        /// Build an app with a hot-reload callback that updates warnings and
        /// memory_config from the in-memory Config after every PUT.
        fn hot_reload_app() -> (std::path::PathBuf, Arc<AppState<MockProvider>>, Router) {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "buddy-hot-reload-{}-{}",
                std::process::id(),
                id
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let config_path = dir.join("buddy.toml");
            let initial_toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#;
            std::fs::write(&config_path, initial_toml).unwrap();
            let config = crate::config::Config::parse(initial_toml).unwrap();

            // Start with the no_embedding_model warning (matches single-provider, no-embedding startup).
            let warnings = new_shared_warnings();
            {
                let mut c = warnings.write().unwrap();
                c.add(Warning {
                    code: "no_embedding_model".into(),
                    message: "No embedding model configured.".into(),
                    severity: WarningSeverity::Warning,
                });
                c.add(Warning {
                    code: "single_chat_provider".into(),
                    message: "Only one chat provider.".into(),
                    severity: WarningSeverity::Info,
                });
            }

            let state = Arc::new(AppState {
                provider: arc_swap::ArcSwap::from_pointee(MockProvider {
                    tokens: vec!["hi".into()],
                }),
                registry: arc_swap::ArcSwap::from_pointee(SkillRegistry::new()),
                store: crate::store::Store::open_in_memory().unwrap(),
                embedder: arc_swap::ArcSwap::from_pointee(None),
                vector_store: arc_swap::ArcSwap::from_pointee(None),
                working_memory: crate::skill::working_memory::new_working_memory_map(),
                memory_config: arc_swap::ArcSwap::from_pointee(
                    crate::config::MemoryConfig::default(),
                ),
                warnings,
                pending_approvals: new_pending_approvals(),
                conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
                approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
                approval_timeout: std::time::Duration::from_secs(1),
                config: std::sync::RwLock::new(config),
                config_path,
                on_config_change: Some(Box::new(|state| {
                    let config = state.config.read().unwrap();

                    // Rebuild skill registry from config.
                    let registry = crate::skill::build_registry(&config.skills);
                    state.registry.store(Arc::new(registry));

                    // Update memory config.
                    state.memory_config.store(Arc::new(config.memory.clone()));

                    // Rebuild approval overrides.
                    let overrides = crate::reload::build_approval_overrides(&config);
                    state.approval_overrides.store(Arc::new(overrides));

                    // Refresh warnings: simulate provider count from config.
                    let provider_count = config.models.chat.providers.len();
                    let has_embedding = config.models.embedding.is_some();
                    let embedder_ref = state.embedder.load();
                    let vs_ref = state.vector_store.load();
                    crate::reload::refresh_warnings(
                        &state.warnings,
                        provider_count,
                        &*embedder_ref,
                        &*vs_ref,
                    );

                    // If embedding was removed, clear the no_embedding_model
                    // (refresh_warnings already handles this). If embedding was
                    // added but we can't construct a real embedder in tests,
                    // manually clear the warning to simulate successful init.
                    if has_embedding {
                        let mut c = state.warnings.write().unwrap();
                        c.clear("no_embedding_model");
                    }

                    Ok(())
                })),
            });
            let router = Router::new()
                .route("/api/chat", post(chat_handler::<MockProvider>))
                .route("/api/warnings", get(get_warnings::<MockProvider>))
                .route("/api/config", get(get_config::<MockProvider>))
                .route(
                    "/api/config/models",
                    put(put_config_models::<MockProvider>),
                )
                .route(
                    "/api/config/skills",
                    put(put_config_skills::<MockProvider>),
                )
                .route("/api/config/chat", put(put_config_chat::<MockProvider>))
                .route(
                    "/api/config/server",
                    put(put_config_server::<MockProvider>),
                )
                .route(
                    "/api/config/memory",
                    put(put_config_memory::<MockProvider>),
                )
                .with_state(state.clone());
            (dir, state, router)
        }

        #[tokio::test]
        async fn put_two_chat_providers_updates_provider_count() {
            let (dir, _state, app) = hot_reload_app();
            let body = serde_json::json!({
                "chat": {
                    "providers": [
                        {
                            "type": "lmstudio",
                            "model": "model-a",
                            "endpoint": "http://localhost:1234/v1"
                        },
                        {
                            "type": "lmstudio",
                            "model": "model-b",
                            "endpoint": "http://localhost:5678/v1"
                        }
                    ]
                }
            });
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            // After reload, single_chat_provider warning should be gone.
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/warnings")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let warnings: Vec<Warning> = serde_json::from_slice(&bytes).unwrap();
            assert!(
                !warnings.iter().any(|w| w.code == "single_chat_provider"),
                "single_chat_provider warning should be cleared after adding 2 providers"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn adding_embedding_clears_warning() {
            let (dir, _state, app) = hot_reload_app();

            // Initially the no_embedding_model warning should exist.
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/warnings")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let warnings: Vec<Warning> = serde_json::from_slice(&bytes).unwrap();
            assert!(
                warnings.iter().any(|w| w.code == "no_embedding_model"),
                "no_embedding_model warning should be present initially"
            );

            // Add an embedding provider via PUT /api/config/models.
            let body = serde_json::json!({
                "chat": {
                    "providers": [{
                        "type": "lmstudio",
                        "model": "test-model",
                        "endpoint": "http://localhost:1234/v1"
                    }]
                },
                "embedding": {
                    "providers": [{
                        "type": "local",
                        "model": "all-minilm"
                    }]
                }
            });
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            // After reload, no_embedding_model warning should be gone.
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/warnings")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let warnings: Vec<Warning> = serde_json::from_slice(&bytes).unwrap();
            assert!(
                !warnings.iter().any(|w| w.code == "no_embedding_model"),
                "no_embedding_model warning should be cleared after adding embedding"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn removing_embedding_adds_warning() {
            let (dir, state, app) = hot_reload_app();

            // Start with embedding "present" — clear the warning manually.
            {
                let mut c = state.warnings.write().unwrap();
                c.clear("no_embedding_model");
            }

            // Write config that has embedding, so we can then remove it.
            let body_with = serde_json::json!({
                "chat": {
                    "providers": [{
                        "type": "lmstudio",
                        "model": "test-model",
                        "endpoint": "http://localhost:1234/v1"
                    }]
                },
                "embedding": {
                    "providers": [{
                        "type": "local",
                        "model": "all-minilm"
                    }]
                }
            });
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body_with).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            // Now remove embedding.
            let body_without = serde_json::json!({
                "chat": {
                    "providers": [{
                        "type": "lmstudio",
                        "model": "test-model",
                        "endpoint": "http://localhost:1234/v1"
                    }]
                }
            });
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/models")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body_without).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            // After reload, no_embedding_model warning should appear.
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/api/warnings")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let warnings: Vec<Warning> = serde_json::from_slice(&bytes).unwrap();
            assert!(
                warnings.iter().any(|w| w.code == "no_embedding_model"),
                "no_embedding_model warning should appear after removing embedding"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn skill_sandbox_rules_updated_after_reload() {
            let (dir, state, app) = hot_reload_app();

            // Initially no skills are registered.
            {
                let reg = state.registry.load();
                assert!(reg.get("read_file").is_none());
            }

            // Update skills config to add read_file.
            let tmp = std::env::temp_dir();
            let body = serde_json::json!({
                "read_file": {
                    "allowed_directories": [tmp.to_str().unwrap()]
                }
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/skills")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            // After reload, the read_file skill should be registered.
            let reg = state.registry.load();
            assert!(
                reg.get("read_file").is_some(),
                "read_file skill should be registered after skills config update"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn system_prompt_change_reflected_in_config() {
            let (dir, state, app) = hot_reload_app();

            let body = serde_json::json!({
                "system_prompt": "You are a pirate assistant."
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/chat")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let config = state.config.read().unwrap();
            assert_eq!(
                config.chat.system_prompt, "You are a pirate assistant.",
                "system prompt should be updated in live config"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn server_config_change_returns_restart_note() {
            let (dir, _state, app) = hot_reload_app();

            let body = serde_json::json!({
                "host": "0.0.0.0",
                "port": 9999
            });
            let response = app
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/server")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

            // Response should include a notes array with restart-required message.
            let notes = json["notes"].as_array().expect("response should have notes array");
            assert!(
                notes.iter().any(|n| n.as_str().unwrap().contains("restart")),
                "response notes should mention restart requirement"
            );

            std::fs::remove_dir_all(&dir).ok();
        }

        #[tokio::test]
        async fn memory_auto_retrieve_disabled_no_memory_event() {
            let (dir, _state, app) = hot_reload_app();

            // Disable auto_retrieve via config update.
            let body = serde_json::json!({
                "auto_retrieve": false,
                "auto_retrieve_limit": 3,
                "similarity_threshold": 0.7
            });
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/config/memory")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&body).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);

            // Send a chat message.
            let events = crate::testutil::post_chat(app, &crate::testutil::make_chat_body()).await;

            // No MemoryContext event should be emitted.
            assert!(
                !events.iter().any(|e| matches!(e, ChatEvent::MemoryContext { .. })),
                "no MemoryContext event should be emitted when auto_retrieve is false"
            );

            std::fs::remove_dir_all(&dir).ok();
        }
    }
}
