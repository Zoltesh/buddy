use std::path::Path;
use std::sync::Arc;

use axum::routing::{get, post, put};
use axum::Router;
use tokio::signal;
use tower_http::services::ServeDir;

mod api;
mod config;
mod embedding;
mod memory;
mod provider;
mod reload;
mod skill;
pub mod store;
#[cfg(test)]
mod testutil;
mod types;
mod warning;

use api::{approve_handler, chat_handler, clear_memory, create_conversation, delete_conversation, get_config, get_conversation, get_warnings, list_conversations, migrate_memory, new_pending_approvals, put_config_chat, put_config_memory, put_config_models, put_config_server, put_config_skills, test_provider, AppState};
use config::SkillsConfig;
use provider::{AnyProvider, ProviderChain};
use store::Store;

type AppProvider = ProviderChain<AnyProvider>;

#[tokio::main]
async fn main() {
    let (config, config_path) = config::Config::load().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let addr = config.bind_address();
    let db_path = config.storage.database.clone();
    let primary_type = config.models.chat.providers[0].provider_type.clone();
    let primary_model = config.models.chat.providers[0].model.clone();

    // Validate skills configuration before proceeding.
    validate_skills_config(&config.skills);

    let store = Store::open(Path::new(&db_path)).unwrap_or_else(|e| {
        eprintln!("Error: failed to initialize database: {e}");
        std::process::exit(1);
    });

    // Build initial runtime components from config using the reload module.
    let working_memory = skill::working_memory::new_working_memory_map();

    let provider = reload::build_provider_chain(&config).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });
    let provider_count = provider.len();

    let embedder = reload::build_embedder(&config).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let vector_store = reload::build_vector_store(&embedder).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let registry = reload::build_skill_registry(
        &config,
        working_memory.clone(),
        &embedder,
        &vector_store,
    );
    let skill_count = registry.len();

    let approval_overrides = reload::build_approval_overrides(&config);

    // Collect startup warnings.
    let warnings = warning::new_shared_warnings();
    reload::refresh_warnings(&warnings, provider_count, &embedder, &vector_store);

    let state = Arc::new(AppState {
        provider: arc_swap::ArcSwap::from_pointee(provider),
        registry: arc_swap::ArcSwap::from_pointee(registry),
        store,
        embedder: arc_swap::ArcSwap::from_pointee(embedder.clone()),
        vector_store: arc_swap::ArcSwap::from_pointee(vector_store),
        working_memory,
        memory_config: arc_swap::ArcSwap::from_pointee(config.memory.clone()),
        warnings,
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        approval_overrides: arc_swap::ArcSwap::from_pointee(approval_overrides),
        approval_timeout: std::time::Duration::from_secs(60),
        config: std::sync::RwLock::new(config),
        config_path,
        on_config_change: Some(Box::new(|state| {
            let config = state.config.read().unwrap();
            reload::reload_from_config(&config, state).map_err(|e| e.to_string())
        })),
    });

    let app = Router::new()
        .route("/api/chat", post(chat_handler::<AppProvider>))
        .route("/api/conversations", get(list_conversations::<AppProvider>).post(create_conversation::<AppProvider>))
        .route("/api/conversations/{id}", get(get_conversation::<AppProvider>).delete(delete_conversation::<AppProvider>))
        .route("/api/chat/{conversation_id}/approve", post(approve_handler::<AppProvider>))
        .route("/api/memory/migrate", post(migrate_memory::<AppProvider>))
        .route("/api/memory", axum::routing::delete(clear_memory::<AppProvider>))
        .route("/api/warnings", get(get_warnings::<AppProvider>))
        .route("/api/config", get(get_config::<AppProvider>))
        .route("/api/config/models", put(put_config_models::<AppProvider>))
        .route("/api/config/skills", put(put_config_skills::<AppProvider>))
        .route("/api/config/chat", put(put_config_chat::<AppProvider>))
        .route("/api/config/server", put(put_config_server::<AppProvider>))
        .route("/api/config/memory", put(put_config_memory::<AppProvider>))
        .route("/api/config/test-provider", post(test_provider::<AppProvider>))
        .with_state(state)
        .fallback_service(ServeDir::new("frontend/dist"));

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("Error: failed to bind to {addr}: {e}");
            std::process::exit(1);
        });

    let embedder_status = match &embedder {
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
