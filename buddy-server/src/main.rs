use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::signal;
use tower_http::services::ServeDir;

mod api;
mod config;
mod provider;
mod skill;
pub mod store;
mod types;

use api::{chat_handler, create_conversation, delete_conversation, get_conversation, list_conversations, AppState};
use provider::AnyProvider;
use provider::lmstudio::LmStudioProvider;
use provider::openai::OpenAiProvider;
use skill::build_registry;
use store::Store;

#[tokio::main]
async fn main() {
    let config = config::Config::load().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let addr = config.bind_address();
    let model = config.provider.model.clone();
    let provider_type = config.provider.provider_type.clone();
    let db_path = &config.storage.database;

    let store = Store::open(std::path::Path::new(db_path)).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let provider = match provider_type.as_str() {
        "openai" => {
            if config.provider.api_key.is_empty() {
                eprintln!("Error: provider.api_key is required when provider.type = \"openai\"");
                std::process::exit(1);
            }
            AnyProvider::OpenAi(OpenAiProvider::new(&config.provider))
        }
        "lmstudio" => AnyProvider::LmStudio(LmStudioProvider::new(&config.provider)),
        other => {
            eprintln!(
                "Error: unknown provider type '{}'. Valid types: openai, lmstudio",
                other
            );
            std::process::exit(1);
        }
    };

    let registry = build_registry(&config.skills);
    let skill_count = registry.len();
    let state = Arc::new(AppState {
        provider,
        registry,
        store,
    });

    let app = Router::new()
        .route("/api/chat", post(chat_handler::<AnyProvider>))
        .route("/api/conversations", get(list_conversations::<AnyProvider>).post(create_conversation::<AnyProvider>))
        .route("/api/conversations/{id}", get(get_conversation::<AnyProvider>).delete(delete_conversation::<AnyProvider>))
        .with_state(state)
        .fallback_service(ServeDir::new("frontend/dist"));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to bind to {addr}: {e}");
            std::process::exit(1);
        });

    println!("buddy server started");
    println!("  address:  http://{addr}");
    println!("  provider: {provider_type}");
    println!("  model:    {model}");
    println!("  skills:   {skill_count} registered");
    println!("  database: {db_path}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    println!("\nShutting down...");
}
