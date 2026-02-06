use std::path::Path;
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
#[cfg(test)]
mod testutil;
mod types;

use api::{chat_handler, create_conversation, delete_conversation, get_conversation, list_conversations, AppState};
use config::SkillsConfig;
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
    let primary = &config.models.chat.providers[0];
    let model = primary.model.clone();
    let provider_type = primary.provider_type.clone();
    let system_prompt = &config.chat.system_prompt;
    let db_path = &config.storage.database;

    // Validate skills configuration before proceeding.
    validate_skills_config(&config.skills);

    let store = Store::open(Path::new(db_path)).unwrap_or_else(|e| {
        eprintln!("Error: failed to initialize database: {e}");
        std::process::exit(1);
    });

    let endpoint = primary.endpoint.as_deref().unwrap_or_else(|| {
        eprintln!(
            "Error: endpoint is required for provider type '{provider_type}'"
        );
        std::process::exit(1);
    });

    let api_key = primary.resolve_api_key().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let provider = match provider_type.as_str() {
        "openai" => {
            if api_key.is_empty() {
                eprintln!("Error: api_key_env is required when type = \"openai\"");
                std::process::exit(1);
            }
            AnyProvider::OpenAi(OpenAiProvider::new(&api_key, &model, endpoint, system_prompt))
        }
        "lmstudio" => {
            AnyProvider::LmStudio(LmStudioProvider::new(&model, endpoint, system_prompt))
        }
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

/// Validate skill sandbox configuration at startup.
///
/// Checks that all configured allowed directories exist and are actual
/// directories. Prints warnings for any that don't exist but does not
/// abort — the skills will still enforce path validation at runtime.
fn validate_skills_config(skills: &SkillsConfig) {
    if let Some(ref cfg) = skills.read_file {
        for dir in &cfg.allowed_directories {
            let path = Path::new(dir);
            if !path.exists() {
                eprintln!("Warning: skills.read_file allowed directory does not exist: {dir}");
            } else if !path.is_dir() {
                eprintln!("Warning: skills.read_file allowed path is not a directory: {dir}");
            }
        }
    }
    if let Some(ref cfg) = skills.write_file {
        for dir in &cfg.allowed_directories {
            let path = Path::new(dir);
            if !path.exists() {
                eprintln!("Warning: skills.write_file allowed directory does not exist: {dir}");
            } else if !path.is_dir() {
                eprintln!("Warning: skills.write_file allowed path is not a directory: {dir}");
            }
        }
    }
    if let Some(ref cfg) = skills.fetch_url {
        if cfg.allowed_domains.is_empty() {
            eprintln!("Warning: skills.fetch_url is configured but allowed_domains is empty");
        }
    }
}

async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    println!("\nShutting down...");
}
