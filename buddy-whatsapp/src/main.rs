use std::path::Path;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tokio::signal;

mod adapter;
mod client;

const DEFAULT_CONFIG_PATH: &str = "buddy.toml";

struct AppState {
    verify_token: String,
    #[allow(dead_code)]
    client: client::WhatsAppClient,
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let config =
        buddy_core::config::Config::from_file(Path::new(DEFAULT_CONFIG_PATH)).unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });

    let whatsapp = &config.interfaces.whatsapp;
    if !whatsapp.enabled {
        println!("WhatsApp interface is not enabled in config.");
        return;
    }

    let api_token = match std::env::var(&whatsapp.api_token_env) {
        Ok(t) if !t.is_empty() => t,
        _ => {
            eprintln!(
                "Error: environment variable '{}' is not set",
                whatsapp.api_token_env
            );
            std::process::exit(1);
        }
    };

    let wa_client =
        client::WhatsAppClient::new(api_token, whatsapp.phone_number_id.clone());

    let state = Arc::new(AppState {
        verify_token: whatsapp.verify_token.clone(),
        client: wa_client,
    });

    let port = whatsapp.webhook_port;
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

async fn receive_webhook(
    axum::Json(payload): axum::Json<adapter::WebhookPayload>,
) -> StatusCode {
    let messages = adapter::extract_messages(&payload);
    for msg in messages {
        if let Some(buddy_msg) = adapter::whatsapp_to_buddy(msg) {
            let text = match &buddy_msg.content {
                buddy_core::types::MessageContent::Text { text } => text.as_str(),
                _ => continue,
            };
            log::info!("[WhatsApp] from {}: {}", msg.from, text);
        } else {
            log::info!("[WhatsApp] from {}: non-text message ({})", msg.from, msg.message_type);
        }
    }
    StatusCode::OK
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
        Arc::new(AppState {
            verify_token: "test-verify-token".to_string(),
            client: client::WhatsAppClient::new(
                "fake-token".to_string(),
                "fake-phone-id".to_string(),
            ),
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
}
