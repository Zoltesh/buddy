//! Authentication middleware and endpoints.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use buddy_core::provider::Provider;

use super::AppState;

// ── Utility ─────────────────────────────────────────────────────────

/// Hash a plaintext token with SHA-256 and return it in `sha256:<hex>` format.
pub fn hash_token(plaintext: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plaintext.as_bytes());
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

// ── Middleware ───────────────────────────────────────────────────────

/// Returns `true` when the server host is a localhost address and auth should
/// be skipped.
fn is_localhost(host: &str) -> bool {
    host == "127.0.0.1" || host == "localhost" || host == "::1"
}

/// Returns `true` when authentication is required: `token_hash` is configured
/// AND the server is bound to a non-localhost address.
fn auth_required(config: &buddy_core::config::Config) -> bool {
    if is_localhost(&config.server.host) {
        return false;
    }
    config.auth.token_hash.is_some()
}

/// Axum middleware that enforces bearer-token authentication on API routes.
///
/// Returns a boxed future so the concrete return type is known to the compiler,
/// which avoids trait-resolution issues with `from_fn_with_state` on generic
/// `AppState<P>`.
pub fn auth_middleware<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    req: Request,
    next: Next,
) -> Pin<Box<dyn Future<Output = Response> + Send>> {
    // Read everything we need from config synchronously, before any async work.
    let expected_hash = {
        let config = state.config.read().unwrap();
        if !auth_required(&config) {
            None // signals: skip auth
        } else {
            Some(config.auth.token_hash.as_ref().unwrap().clone())
        }
    }; // RwLockReadGuard dropped here

    Box::pin(async move {
        let Some(expected_hash) = expected_hash else {
            return next.run(req).await;
        };

        // Extract the Bearer token from the Authorization header.
        let provided_hash = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(|t| hash_token(t));

        match provided_hash {
            Some(h) if h == expected_hash => next.run(req).await,
            _ => unauthorized_response(),
        }
    })
}

fn unauthorized_response() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "Unauthorized" })),
    )
        .into_response()
}

// ── Endpoints ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub valid: bool,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub required: bool,
}

/// `POST /api/auth/verify` — check whether a plaintext token is valid.
///
/// Exempt from auth middleware. If auth is disabled, always returns `valid: true`.
pub async fn verify_token<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(body): Json<VerifyRequest>,
) -> Json<VerifyResponse> {
    let config = state.config.read().unwrap();

    if !auth_required(&config) {
        return Json(VerifyResponse { valid: true });
    }

    let expected_hash = config.auth.token_hash.as_ref().unwrap();
    let provided_hash = hash_token(&body.token);
    Json(VerifyResponse {
        valid: provided_hash == *expected_hash,
    })
}

/// `GET /api/auth/status` — report whether authentication is required.
///
/// Exempt from auth middleware.
pub async fn auth_status<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<StatusResponse> {
    let config = state.config.read().unwrap();
    Json(StatusResponse {
        required: auth_required(&config),
    })
}
