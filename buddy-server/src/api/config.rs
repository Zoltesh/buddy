//! Configuration read/write endpoints and validation.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::{ApiError, AppState};
use crate::provider::Provider;
use url::Url;

// ── Validation types and helpers ────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ValidationErrorResponse {
    pub errors: Vec<FieldError>,
}

fn validate_provider(
    p: &crate::config::ProviderEntry,
    prefix: &str,
    i: usize,
    errors: &mut Vec<FieldError>,
) {
    if !["openai", "lmstudio", "local"].contains(&p.provider_type.as_str()) {
        errors.push(FieldError {
            field: format!("{prefix}[{i}].type"),
            message: format!(
                "unknown provider type '{}'; expected openai, lmstudio, or local",
                p.provider_type
            ),
        });
    }
    if p.model.is_empty() {
        errors.push(FieldError {
            field: format!("{prefix}[{i}].model"),
            message: "must not be empty".into(),
        });
    }
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
        validate_provider(p, "models.chat.providers", i, &mut errors);
    }
    if let Some(ref emb) = models.embedding {
        for (i, p) in emb.providers.iter().enumerate() {
            validate_provider(p, "models.embedding.providers", i, &mut errors);
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

pub(crate) fn validate_skills(skills: &crate::config::SkillsConfig) -> Vec<FieldError> {
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

// ── Config persistence helpers ──────────────────────────────────────────

fn atomic_write(path: &std::path::Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "config path has no parent directory".to_string())?;
    let tmp_path = parent.join(".buddy.toml.tmp");
    std::fs::write(&tmp_path, content)
        .map_err(|e| format!("failed to write temp file: {e}"))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("failed to rename temp file: {e}"))?;
    Ok(())
}

/// Apply a mutation to the config, persist to disk, and trigger hot-reload.
fn apply_config_update<P: Provider + 'static>(
    state: &Arc<AppState<P>>,
    mutate: impl FnOnce(&mut crate::config::Config),
) -> Result<crate::config::Config, axum::response::Response> {
    {
        let mut config = state.config.write().unwrap();
        mutate(&mut config);
        let toml = config.to_toml_string();
        if let Err(e) = atomic_write(&state.config_path, &toml) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError { code: "write_error".into(), message: e }),
            ).into_response());
        }
    }
    if let Some(ref hook) = state.on_config_change {
        if let Err(e) = hook(state) {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "reload_failed".into(),
                    message: format!("config saved but reload failed: {e}"),
                }),
            ).into_response());
        }
    }
    let config = state.config.read().unwrap();
    Ok(config.clone())
}

// ── Config read/write handlers ──────────────────────────────────────────

/// `GET /api/config` — return the current configuration as JSON.
pub async fn get_config<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<crate::config::Config> {
    let config = state.config.read().unwrap();
    Json(config.clone())
}

/// `PUT /api/config/models` — update the models section.
pub async fn put_config_models<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(models): Json<crate::config::ModelsConfig>,
) -> axum::response::Response {
    let errors = validate_models(&models);
    if !errors.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ValidationErrorResponse { errors })).into_response();
    }
    match apply_config_update(&state, |config| config.models = models) {
        Ok(config) => Json(config).into_response(),
        Err(resp) => resp,
    }
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
    match apply_config_update(&state, |config| config.skills = skills) {
        Ok(config) => Json(config).into_response(),
        Err(resp) => resp,
    }
}

/// `PUT /api/config/chat` — update the chat section.
pub async fn put_config_chat<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(chat): Json<crate::config::ChatConfig>,
) -> axum::response::Response {
    match apply_config_update(&state, |config| config.chat = chat) {
        Ok(config) => Json(config).into_response(),
        Err(resp) => resp,
    }
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
    match apply_config_update(&state, |config| config.memory = memory) {
        Ok(config) => Json(config).into_response(),
        Err(resp) => resp,
    }
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
            message: "an API key is required when type = \"openai\"".into(),
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

// ── Model discovery ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DiscoverModelsRequest {
    pub endpoint: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct DiscoveredModel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscoverModelsResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<DiscoveredModel>>,
}

/// Extract the origin (scheme + host + port) from an endpoint URL.
fn base_url(endpoint: &str) -> Result<Url, String> {
    let mut url = Url::parse(endpoint).map_err(|e| format!("invalid endpoint URL: {e}"))?;
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

/// Try the native LM Studio REST API (`/api/v0/models`).
async fn try_native_discovery(
    client: &reqwest::Client,
    base: &Url,
) -> Option<Vec<DiscoveredModel>> {
    let url = format!("{}api/v0/models", base.as_str());
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let data = json.get("data")?.as_array()?;
    let models = data
        .iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            let loaded = m
                .get("state")
                .and_then(|s| s.as_str())
                .map(|s| s == "loaded");
            let context_length = m
                .get("max_context_length")
                .and_then(|v| v.as_u64());
            Some(DiscoveredModel {
                id,
                loaded,
                context_length,
            })
        })
        .collect();
    Some(models)
}

/// Fallback: try the OpenAI-compatible `/models` endpoint.
async fn try_openai_discovery(
    client: &reqwest::Client,
    endpoint: &str,
) -> Option<Vec<DiscoveredModel>> {
    let url = format!("{}/models", endpoint.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let data = json.get("data")?.as_array()?;
    let models = data
        .iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            Some(DiscoveredModel {
                id,
                loaded: None,
                context_length: None,
            })
        })
        .collect();
    Some(models)
}

/// `POST /api/config/discover-models` — query an LM Studio endpoint for available models.
pub async fn discover_models<P: Provider + 'static>(
    State(_state): State<Arc<AppState<P>>>,
    Json(req): Json<DiscoverModelsRequest>,
) -> Json<DiscoverModelsResponse> {
    if req.endpoint.trim().is_empty() {
        return Json(DiscoverModelsResponse {
            status: "error".into(),
            message: Some("endpoint is required".into()),
            models: None,
        });
    }

    let base = match base_url(&req.endpoint) {
        Ok(b) => b,
        Err(e) => {
            return Json(DiscoverModelsResponse {
                status: "error".into(),
                message: Some(e),
                models: None,
            });
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return Json(DiscoverModelsResponse {
                status: "error".into(),
                message: Some(format!("failed to build HTTP client: {e}")),
                models: None,
            });
        }
    };

    // Try native LM Studio API first for richer metadata.
    if let Some(models) = try_native_discovery(&client, &base).await {
        return Json(DiscoverModelsResponse {
            status: "ok".into(),
            message: None,
            models: Some(models),
        });
    }

    // Fall back to OpenAI-compatible /models endpoint.
    if let Some(models) = try_openai_discovery(&client, &req.endpoint).await {
        return Json(DiscoverModelsResponse {
            status: "ok".into(),
            message: None,
            models: Some(models),
        });
    }

    Json(DiscoverModelsResponse {
        status: "error".into(),
        message: Some("Connection failed: could not reach the models endpoint".into()),
        models: None,
    })
}
