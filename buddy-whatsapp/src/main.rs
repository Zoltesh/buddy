use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use tokio::signal;

use buddy_core::provider::{AnyProvider, ProviderChain};
use buddy_core::state::AppState as CoreState;

mod adapter;
mod approval;
mod client;
mod conversation;

const DEFAULT_CONFIG_PATH: &str = "buddy.toml";

/// TTL for duplicate message filtering.
const DEDUP_TTL: Duration = Duration::from_secs(300);

/// In-memory deduplication filter for WhatsApp webhook messages.
///
/// WhatsApp may deliver the same webhook event multiple times. This filter
/// tracks recently seen message IDs and rejects duplicates within a 5-minute
/// window.
struct MessageDedup {
    seen: Mutex<HashMap<String, Instant>>,
}

impl MessageDedup {
    fn new() -> Self {
        Self {
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `true` if this message_id has not been seen recently
    /// (i.e. it should be processed). Inserts the ID with the current
    /// timestamp and lazily evicts expired entries.
    fn check_and_insert(&self, message_id: &str) -> bool {
        let mut seen = self.seen.lock().unwrap();
        let now = Instant::now();
        seen.retain(|_, ts| now.duration_since(*ts) < DEDUP_TTL);
        if seen.contains_key(message_id) {
            false
        } else {
            seen.insert(message_id.to_string(), now);
            true
        }
    }
}

struct AppState {
    core: CoreState<ProviderChain<AnyProvider>>,
    client: client::WhatsAppClient,
    verify_token: String,
    app_secret: Option<String>,
    dedup: MessageDedup,
    pending_approvals: approval::WhatsAppPendingApprovals,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config =
        buddy_core::config::Config::from_file(Path::new(DEFAULT_CONFIG_PATH)).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });

    if !config.interfaces.whatsapp.enabled {
        println!("WhatsApp interface is not enabled in config.");
        return;
    }

    let api_token_env = config.interfaces.whatsapp.api_token_env.clone();
    let phone_number_id = config.interfaces.whatsapp.phone_number_id.clone();
    let verify_token = config.interfaces.whatsapp.verify_token.clone();
    let port = config.interfaces.whatsapp.webhook_port;

    let api_token = match std::env::var(&api_token_env) {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!("Error: environment variable '{api_token_env}' is not set");
            std::process::exit(1);
        }
    };

    let app_secret_env = &config.interfaces.whatsapp.app_secret_env;
    let app_secret = match std::env::var(app_secret_env) {
        Ok(s) if !s.is_empty() => Some(s),
        _ => {
            log::warn!(
                "Environment variable '{app_secret_env}' is not set; \
                 webhook signature verification is disabled"
            );
            None
        }
    };

    let wa_client = client::WhatsAppClient::new(api_token, phone_number_id);

    let core = CoreState::new(config, Path::new(DEFAULT_CONFIG_PATH)).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let state = Arc::new(AppState {
        core,
        client: wa_client,
        verify_token,
        app_secret,
        dedup: MessageDedup::new(),
        pending_approvals: approval::new_whatsapp_pending_approvals(),
    });

    let app = create_router(state);

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to bind to port {port}: {e}");
            std::process::exit(1);
        });

    println!("buddy-whatsapp webhook listening on 127.0.0.1:{port}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: server error: {e}");
            std::process::exit(1);
        });
}

fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/webhook", get(verify_webhook).post(receive_webhook))
        .with_state(state)
}

#[derive(Deserialize)]
struct VerifyQuery {
    #[serde(rename = "hub.mode")]
    hub_mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    hub_verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    hub_challenge: Option<String>,
}

async fn verify_webhook(
    State(state): State<Arc<AppState>>,
    Query(query): Query<VerifyQuery>,
) -> impl IntoResponse {
    let mode = query.hub_mode.as_deref().unwrap_or("");
    let token = query.hub_verify_token.as_deref().unwrap_or("");
    let challenge = query.hub_challenge.as_deref().unwrap_or("");

    if mode == "subscribe" && token == state.verify_token {
        log::info!("Webhook verification succeeded");
        (StatusCode::OK, challenge.to_string())
    } else {
        log::warn!("Webhook verification failed: mode={mode}, token mismatch");
        (StatusCode::FORBIDDEN, String::new())
    }
}

/// Verify the HMAC-SHA256 signature from the `X-Hub-Signature-256` header.
///
/// Returns `true` if the signature is valid, `false` otherwise.
fn verify_signature(secret: &str, body: &[u8], header_value: &str) -> bool {
    let hex_digest = match header_value.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };
    let expected = match hex::decode(hex_digest) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key size");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

async fn receive_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    if let Some(secret) = &state.app_secret {
        let signature = match headers.get("x-hub-signature-256").and_then(|v| v.to_str().ok()) {
            Some(s) => s,
            None => return StatusCode::FORBIDDEN,
        };
        if !verify_signature(secret, &body, signature) {
            return StatusCode::FORBIDDEN;
        }
    }

    let payload: adapter::WebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            log::warn!("Failed to deserialize webhook payload: {e}");
            return StatusCode::BAD_REQUEST;
        }
    };

    let messages = adapter::extract_messages(&payload);
    for msg in messages {
        // Handle button replies for skill approval.
        if let Some((phone, button_id)) = adapter::extract_button_reply(msg) {
            let approved = button_id == approval::BUTTON_APPROVE;
            log::info!(
                "[WhatsApp] button reply from {}: {} ({})",
                phone,
                button_id,
                if approved { "approved" } else { "denied" }
            );
            let sender = {
                let mut guard = state.pending_approvals.lock().await;
                guard.remove(phone)
            };
            if let Some(tx) = sender {
                let _ = tx.send(approved);
            }
            continue;
        }

        if msg.message_type != "text" {
            log::info!(
                "[WhatsApp] from {}: non-text message ({})",
                msg.from,
                msg.message_type
            );
            continue;
        }
        let text = match msg.text.as_ref() {
            Some(t) => t.body.clone(),
            None => continue,
        };

        if !state.dedup.check_and_insert(&msg.id) {
            log::debug!(
                "[WhatsApp] duplicate message {} from {}, skipping",
                msg.id,
                msg.from
            );
            continue;
        }

        log::info!("[WhatsApp] from {}: {}", msg.from, text);

        let state = Arc::clone(&state);
        let phone = msg.from.clone();
        tokio::spawn(async move {
            process_incoming_message(&state, &phone, &text).await;
        });
    }
    StatusCode::OK
}

async fn process_incoming_message(state: &AppState, phone: &str, text: &str) {
    let provider = state.core.provider.load();
    let registry = state.core.registry.load();
    let approval_overrides = state.core.approval_overrides.load();

    let approval_ctx = conversation::WhatsAppApprovalContext {
        client: &state.client,
        phone,
        pending: &state.pending_approvals,
        timeout: state.core.approval_timeout,
    };

    let result = conversation::process_message(
        &state.core.store,
        &**provider,
        &registry,
        &approval_overrides,
        &state.core.conversation_approvals,
        phone,
        text,
        Some(approval_ctx),
    )
    .await;

    let (final_text, tool_results) = match result {
        Ok(conversation::ProcessResult::Response {
            final_text,
            tool_results,
        }) => (final_text, tool_results),
        Ok(conversation::ProcessResult::Empty) => return,
        Err(e) => {
            let _ = state
                .client
                .send_text_message(phone, e.user_message())
                .await;
            return;
        }
    };

    // Send tool results first, then the final text.
    for part in tool_results {
        for chunk in adapter::split_message(&part) {
            if let Err(e) = state.client.send_text_message(phone, &chunk).await {
                log::error!("Failed to send WhatsApp message to {phone}: {e}");
            }
        }
    }

    let response_text = adapter::markdown_to_whatsapp(&final_text);
    let parts = adapter::split_message(&response_text);
    for part in parts {
        if let Err(e) = state.client.send_text_message(phone, &part).await {
            log::error!("Failed to send WhatsApp message to {phone}: {e}");
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => log::info!("Received Ctrl+C, shutting down"),
        () = terminate => log::info!("Received SIGTERM, shutting down"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_state() -> Arc<AppState> {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test"
endpoint = "http://127.0.0.1:1"

[[models.embedding.providers]]
type = "openai"
model = "text-embedding-3-small"

[storage]
database = ":memory:"
"#,
        )
        .unwrap();

        let core = CoreState::new(config, Path::new("/tmp/buddy-whatsapp-test.toml")).unwrap();

        Arc::new(AppState {
            core,
            client: client::WhatsAppClient::new(
                "fake-token".to_string(),
                "fake-phone-id".to_string(),
            ),
            verify_token: "test-verify-token".to_string(),
            app_secret: None,
            dedup: MessageDedup::new(),
            pending_approvals: approval::new_whatsapp_pending_approvals(),
        })
    }

    fn test_state_with_secret(secret: &str) -> Arc<AppState> {
        let config = buddy_core::config::Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test"
endpoint = "http://127.0.0.1:1"

[[models.embedding.providers]]
type = "openai"
model = "text-embedding-3-small"

[storage]
database = ":memory:"
"#,
        )
        .unwrap();

        let core = CoreState::new(config, Path::new("/tmp/buddy-whatsapp-test.toml")).unwrap();

        Arc::new(AppState {
            core,
            client: client::WhatsAppClient::new(
                "fake-token".to_string(),
                "fake-phone-id".to_string(),
            ),
            verify_token: "test-verify-token".to_string(),
            app_secret: Some(secret.to_string()),
            dedup: MessageDedup::new(),
            pending_approvals: approval::new_whatsapp_pending_approvals(),
        })
    }

    #[tokio::test]
    async fn webhook_verification_succeeds_with_correct_token() {
        let app = create_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/webhook?hub.mode=subscribe&hub.verify_token=test-verify-token&hub.challenge=abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"abc123");
    }

    #[tokio::test]
    async fn webhook_verification_fails_with_wrong_token() {
        let app = create_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/webhook?hub.mode=subscribe&hub.verify_token=wrong-token&hub.challenge=abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn webhook_verification_fails_with_wrong_mode() {
        let app = create_router(test_state());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/webhook?hub.mode=unsubscribe&hub.verify_token=test-verify-token&hub.challenge=abc123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn receive_webhook_text_message_returns_200() {
        let app = create_router(test_state());
        let payload = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "BIZ_ID",
                "changes": [{
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "15551234567",
                            "phone_number_id": "PHONE_ID"
                        },
                        "messages": [{
                            "id": "wamid.test789",
                            "from": "15559876543",
                            "timestamp": "1700000000",
                            "type": "text",
                            "text": { "body": "Hello from WhatsApp" }
                        }]
                    },
                    "field": "messages"
                }]
            }]
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn receive_webhook_empty_payload_returns_200() {
        let app = create_router(test_state());
        let payload = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": []
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn dedup_filters_duplicate_messages() {
        let dedup = MessageDedup::new();
        assert!(dedup.check_and_insert("wamid.test789"));
        assert!(!dedup.check_and_insert("wamid.test789"));
        assert!(dedup.check_and_insert("wamid.test790"));
    }

    /// Compute `sha256=<hex>` signature for a payload, matching Meta's format.
    fn sign_payload(secret: &str, body: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    }

    fn sample_webhook_body() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "BIZ_ID",
                "changes": [{
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "15551234567",
                            "phone_number_id": "PHONE_ID"
                        },
                        "messages": [{
                            "id": "wamid.sig_test",
                            "from": "15559876543",
                            "timestamp": "1700000000",
                            "type": "text",
                            "text": { "body": "Signed message" }
                        }]
                    },
                    "field": "messages"
                }]
            }]
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn valid_signature_passes_verification() {
        let secret = "test-app-secret";
        let app = create_router(test_state_with_secret(secret));
        let body = sample_webhook_body();
        let signature = sign_payload(secret, &body);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .header("x-hub-signature-256", signature)
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_signature_returns_403() {
        let secret = "test-app-secret";
        let app = create_router(test_state_with_secret(secret));
        let body = sample_webhook_body();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .header("x-hub-signature-256", "sha256=deadbeef")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn missing_signature_header_returns_403() {
        let secret = "test-app-secret";
        let app = create_router(test_state_with_secret(secret));
        let body = sample_webhook_body();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn no_secret_configured_accepts_unsigned_requests() {
        // test_state() has app_secret: None, simulating no secret configured.
        let app = create_router(test_state());
        let body = sample_webhook_body();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn webhook_returns_200_within_100ms() {
        let app = create_router(test_state());
        let payload = serde_json::json!({
            "object": "whatsapp_business_account",
            "entry": [{
                "id": "BIZ_ID",
                "changes": [{
                    "value": {
                        "messaging_product": "whatsapp",
                        "metadata": {
                            "display_phone_number": "15551234567",
                            "phone_number_id": "PHONE_ID"
                        },
                        "messages": [{
                            "id": "wamid.timing_test",
                            "from": "15559876543",
                            "timestamp": "1700000000",
                            "type": "text",
                            "text": { "body": "Timing test" }
                        }]
                    },
                    "field": "messages"
                }]
            }]
        });

        let start = Instant::now();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            elapsed < Duration::from_millis(100),
            "webhook should return within 100ms, took {:?}",
            elapsed
        );
    }
}
