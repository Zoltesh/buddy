use std::path::PathBuf;
use std::sync::Arc;

use axum::routing::{get, post, put};
use axum::Router;
use clap::Parser;
use tokio::signal;
use tower_http::services::{ServeDir, ServeFile};

mod api;
mod process;
mod reload;

#[cfg(test)]
mod testutil;

const DEFAULT_CONFIG_PATH: &str = "buddy.toml";

#[derive(Parser)]
#[command(name = "buddy-server")]
struct Cli {
    /// Path to the configuration file
    #[arg(long = "config", default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
}

fn load_config() -> Result<(buddy_core::config::Config, PathBuf), String> {
    let cli = Cli::parse();
    let config = buddy_core::config::Config::from_file(&cli.config)?;
    Ok((config, cli.config))
}

use api::{approve_handler, chat_handler, check_interface_connection, clear_memory, create_conversation, delete_conversation, discover_models, get_config, get_conversation, get_embedder_health, get_interfaces_status, get_memory_status, get_warnings, list_conversations, migrate_memory, put_config_chat, put_config_interfaces, put_config_memory, put_config_models, put_config_server, put_config_skills, test_provider};
use api::auth::{auth_middleware, auth_status, verify_token};
use buddy_core::provider::{AnyProvider, ProviderChain};
use buddy_core::state::AppState;

type AppProvider = ProviderChain<AnyProvider>;

#[tokio::main]
async fn main() {
    let (config, config_path) = load_config().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let addr = config.bind_address();
    let db_path = config.storage.database.clone();
    let primary_type = config.models.chat.providers[0].provider_type.clone();
    let primary_model = config.models.chat.providers[0].model.clone();

    let mut app_state = AppState::new(config, &config_path).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let provider_count = app_state.provider.load().len();
    let skill_count = app_state.registry.load().len();
    let embedder = app_state.embedder.load_full();

    app_state.on_config_change = Some(Box::new(|state| {
        let config = state.config.read().unwrap();
        reload::reload_from_config(&config, state).map_err(|e| e.to_string())?;
        drop(config);
        process::manage_telegram_on_config_change(state);
        Ok(())
    }));

    let state = Arc::new(app_state);

    // Spawn buddy-telegram if enabled.
    process::manage_telegram(&state);

    // Routes protected by auth middleware.
    let protected_api = Router::new()
        .route("/api/chat", post(chat_handler::<AppProvider>))
        .route("/api/conversations", get(list_conversations::<AppProvider>).post(create_conversation::<AppProvider>))
        .route("/api/conversations/{id}", get(get_conversation::<AppProvider>).delete(delete_conversation::<AppProvider>))
        .route("/api/chat/{conversation_id}/approve", post(approve_handler::<AppProvider>))
        .route("/api/memory/migrate", post(migrate_memory::<AppProvider>))
        .route("/api/memory/status", get(get_memory_status::<AppProvider>))
        .route("/api/memory", axum::routing::delete(clear_memory::<AppProvider>))
        .route("/api/embedder/health", get(get_embedder_health::<AppProvider>))
        .route("/api/warnings", get(get_warnings::<AppProvider>))
        .route("/api/config", get(get_config::<AppProvider>))
        .route("/api/config/models", put(put_config_models::<AppProvider>))
        .route("/api/config/skills", put(put_config_skills::<AppProvider>))
        .route("/api/config/chat", put(put_config_chat::<AppProvider>))
        .route("/api/config/server", put(put_config_server::<AppProvider>))
        .route("/api/config/memory", put(put_config_memory::<AppProvider>))
        .route("/api/config/interfaces", put(put_config_interfaces::<AppProvider>))
        .route("/api/interfaces/status", get(get_interfaces_status::<AppProvider>))
        .route("/api/interfaces/check", post(check_interface_connection::<AppProvider>))
        .route("/api/config/test-provider", post(test_provider::<AppProvider>))
        .route("/api/config/discover-models", post(discover_models::<AppProvider>))
        .route_layer(axum::middleware::from_fn_with_state(state.clone(), auth_middleware::<AppProvider>))
        .with_state(state.clone());

    // Auth endpoints — exempt from auth middleware.
    let public_api = Router::new()
        .route("/api/auth/verify", post(verify_token::<AppProvider>))
        .route("/api/auth/status", get(auth_status::<AppProvider>))
        .with_state(state.clone());

    let app = Router::new()
        .merge(protected_api)
        .merge(public_api)
        .fallback_service(
            ServeDir::new("frontend/dist")
                .fallback(ServeFile::new("frontend/dist/index.html")),
        );

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to bind to {addr}: {e}");
            std::process::exit(1);
        });

    let embedder_status = match embedder.as_ref() {
        Some(e) => format!("{} ({}d)", e.model_name(), e.dimensions()),
        None => "none".into(),
    };

    println!("buddy server started");
    println!("  address:    http://{addr}");
    println!("  provider:   {primary_type}");
    println!("  model:      {primary_model}");
    println!("  chain:      {provider_count} provider(s)");
    println!("  embedder:   {embedder_status}");
    println!("  skills:     {skill_count} registered");
    println!("  database:   {db_path}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    // Clean up child processes on shutdown.
    process::stop_telegram(&state.telegram_process);
}

async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    println!("\nShutting down...");
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn spa_fallback_serves_index_html_for_unknown_routes() {
        let tmp = std::env::temp_dir().join("buddy_test_033_spa");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("index.html"),
            "<!doctype html><html><body>buddy spa</body></html>",
        )
        .unwrap();

        let app: Router = Router::new().fallback_service(
            ServeDir::new(&tmp).fallback(ServeFile::new(tmp.join("index.html"))),
        );

        let req = axum::http::Request::builder()
            .uri("/settings")
            .body(axum::body::Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("buddy spa"));

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
