//! Interface status and configuration endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

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

/// `GET /api/interfaces/status` — return the configuration and enabled status of each interface.
pub async fn get_interfaces_status<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<InterfacesStatusResponse> {
    let config = state.config.read().unwrap();

    let tg = &config.interfaces.telegram;
    let tg_configured = tg.enabled || tg.bot_token_env != "TELEGRAM_BOT_TOKEN";

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
