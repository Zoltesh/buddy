//! Interface status and configuration endpoints.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::AppState;
use super::config::apply_config_update;
use buddy_core::provider::Provider;

#[derive(Serialize)]
pub struct InterfaceStatus {
    pub configured: bool,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct InterfacesStatusResponse {
    pub telegram: InterfaceStatus,
    pub whatsapp: InterfaceStatus,
}

#[derive(Deserialize)]
pub struct CheckRequest {
    pub interface: String,
}

#[derive(Serialize)]
pub struct CheckResponse {
    pub status: String,
    pub detail: String,
}

/// `GET /api/interfaces/status` — return the configuration and enabled status of each interface.
pub async fn get_interfaces_status<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<InterfacesStatusResponse> {
    let config = state.config.read().unwrap();

    let tg = &config.interfaces.telegram;
    let tg_configured = tg.enabled
        || tg.bot_token.as_ref().is_some_and(|t| !t.is_empty())
        || tg.bot_token_env != "TELEGRAM_BOT_TOKEN";

    let wa = &config.interfaces.whatsapp;
    let wa_configured = wa.enabled || !wa.phone_number_id.is_empty();

    Json(InterfacesStatusResponse {
        telegram: InterfaceStatus {
            configured: tg_configured,
            enabled: tg.enabled,
        },
        whatsapp: InterfaceStatus {
            configured: wa_configured,
            enabled: wa.enabled,
        },
    })
}

/// `PUT /api/config/interfaces` — update the interfaces section of config.
pub async fn put_config_interfaces<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(interfaces): Json<buddy_core::config::InterfacesConfig>,
) -> axum::response::Response {
    match apply_config_update(&state, |config| config.interfaces = interfaces) {
        Ok(config) => Json(config).into_response(),
        Err(resp) => resp,
    }
}

const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// `POST /api/interfaces/check` — validate interface credentials against external APIs.
pub async fn check_interface_connection<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Json(body): Json<CheckRequest>,
) -> axum::response::Response {
    match body.interface.as_str() {
        "telegram" => check_telegram(&state).await.into_response(),
        "whatsapp" => check_whatsapp(&state).await.into_response(),
        _ => (
            StatusCode::BAD_REQUEST,
            Json(super::ApiError {
                code: "bad_request".into(),
                message: format!("Unknown interface: {}", body.interface),
            }),
        )
            .into_response(),
    }
}

async fn check_telegram<P: Provider + 'static>(
    state: &Arc<AppState<P>>,
) -> Json<CheckResponse> {
    let token = {
        let config = state.config.read().unwrap();
        config.interfaces.telegram.resolve_bot_token()
    };
    let token = match token {
        Ok(t) => t,
        Err(e) => {
            return Json(CheckResponse {
                status: "error".into(),
                detail: if e.contains("not set") {
                    "Not configured".into()
                } else {
                    e
                },
            });
        }
    };

    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .unwrap();

    let url = format!("https://api.telegram.org/bot{token}/getMe");
    match client.get(&url).send().await {
        Ok(resp) => {
            let json: serde_json::Value = match resp.json().await {
                Ok(v) => v,
                Err(_) => {
                    return Json(CheckResponse {
                        status: "error".into(),
                        detail: "Invalid response from Telegram API".into(),
                    });
                }
            };
            if json.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                let username = json
                    .pointer("/result/username")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                Json(CheckResponse {
                    status: "connected".into(),
                    detail: format!("Bot: @{username}"),
                })
            } else {
                Json(CheckResponse {
                    status: "error".into(),
                    detail: "Invalid bot token".into(),
                })
            }
        }
        Err(e) => {
            if e.is_timeout() {
                Json(CheckResponse {
                    status: "error".into(),
                    detail: "Could not reach Telegram API".into(),
                })
            } else if e.is_connect() {
                Json(CheckResponse {
                    status: "error".into(),
                    detail: "Could not reach Telegram API".into(),
                })
            } else {
                Json(CheckResponse {
                    status: "error".into(),
                    detail: "Could not reach Telegram API".into(),
                })
            }
        }
    }
}

async fn check_whatsapp<P: Provider + 'static>(
    state: &Arc<AppState<P>>,
) -> Json<CheckResponse> {
    let (token, phone_number_id) = {
        let config = state.config.read().unwrap();
        let wa = &config.interfaces.whatsapp;
        if wa.phone_number_id.is_empty() {
            return Json(CheckResponse {
                status: "error".into(),
                detail: "Not configured".into(),
            });
        }
        let token = std::env::var(&wa.api_token_env).ok();
        (token, wa.phone_number_id.clone())
    };

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            return Json(CheckResponse {
                status: "error".into(),
                detail: "Not configured".into(),
            });
        }
    };

    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .unwrap();

    let url = format!("https://graph.facebook.com/v22.0/{phone_number_id}");
    match client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
    {
        Ok(resp) => {
            let status_code = resp.status();
            if status_code.is_success() {
                Json(CheckResponse {
                    status: "connected".into(),
                    detail: format!("Phone: {phone_number_id}"),
                })
            } else if status_code == reqwest::StatusCode::UNAUTHORIZED
                || status_code == reqwest::StatusCode::FORBIDDEN
            {
                Json(CheckResponse {
                    status: "error".into(),
                    detail: "Invalid API token".into(),
                })
            } else {
                Json(CheckResponse {
                    status: "error".into(),
                    detail: "Invalid API token".into(),
                })
            }
        }
        Err(_) => Json(CheckResponse {
            status: "error".into(),
            detail: "Could not reach WhatsApp API".into(),
        }),
    }
}
