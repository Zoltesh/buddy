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
mod skill;
pub mod store;
#[cfg(test)]
mod testutil;
mod types;
mod warning;

use api::{approve_handler, chat_handler, clear_memory, create_conversation, delete_conversation, get_config, get_conversation, get_warnings, list_conversations, migrate_memory, new_pending_approvals, put_config_chat, put_config_memory, put_config_models, put_config_server, put_config_skills, AppState};
use config::SkillsConfig;
use provider::{AnyProvider, ProviderChain};
use provider::lmstudio::LmStudioProvider;
use provider::openai::OpenAiProvider;
use skill::build_registry;
use store::Store;

type AppProvider = ProviderChain<AnyProvider>;

#[tokio::main]
async fn main() {
    let (config, config_path) = config::Config::load().unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    let addr = config.bind_address();
    let system_prompt = config.chat.system_prompt.clone();
    let db_path = config.storage.database.clone();

    // Validate skills configuration before proceeding.
    validate_skills_config(&config.skills);

    let store = Store::open(Path::new(&db_path)).unwrap_or_else(|e| {
        eprintln!("Error: failed to initialize database: {e}");
        std::process::exit(1);
    });

    // Build a provider chain from all configured chat providers.
    let mut chain_entries: Vec<(AnyProvider, String)> = Vec::new();
    for entry in &config.models.chat.providers {
        let endpoint = entry.endpoint.as_deref().unwrap_or_else(|| {
            eprintln!(
                "Error: endpoint is required for provider type '{}'",
                entry.provider_type
            );
            std::process::exit(1);
        });

        let api_key = entry.resolve_api_key().unwrap_or_else(|e| {
            eprintln!("Error: {e}");
            std::process::exit(1);
        });

        let provider = match entry.provider_type.as_str() {
            "openai" => {
                if api_key.is_empty() {
                    eprintln!("Error: api_key_env is required when type = \"openai\"");
                    std::process::exit(1);
                }
                AnyProvider::OpenAi(OpenAiProvider::new(
                    &api_key,
                    &entry.model,
                    endpoint,
                    &system_prompt,
                ))
            }
            "lmstudio" => {
                AnyProvider::LmStudio(LmStudioProvider::new(
                    &entry.model,
                    endpoint,
                    &system_prompt,
                ))
            }
            other => {
                eprintln!(
                    "Error: unknown provider type '{}'. Valid types: openai, lmstudio",
                    other
                );
                std::process::exit(1);
            }
        };
        chain_entries.push((provider, entry.model.clone()));
    }

    let primary_type = config.models.chat.providers[0].provider_type.clone();
    let primary_model = config.models.chat.providers[0].model.clone();
    let provider_count = chain_entries.len();
    let provider = ProviderChain::new(chain_entries);

    // Build the optional embedder from config.
    let embedder: Option<Arc<dyn embedding::Embedder>> = config
        .models
        .embedding
        .as_ref()
        .and_then(|slot| {
            slot.providers
                .iter()
                .find(|e| e.provider_type == "local")
        })
        .map(|_| {
            let e = embedding::local::LocalEmbedder::new().unwrap_or_else(|e| {
                eprintln!("Error: failed to initialize local embedder: {e}");
                std::process::exit(1);
            });
            Arc::new(e) as Arc<dyn embedding::Embedder>
        });

    // Build the optional vector store when an embedder is available.
    let vector_store: Option<Arc<dyn memory::VectorStore>> = embedder.as_ref().map(|e| {
        let vs = memory::sqlite::SqliteVectorStore::open(
            Path::new("memory.db"),
            e.model_name(),
            e.dimensions(),
        )
        .unwrap_or_else(|err| {
            eprintln!("Error: failed to initialize vector store: {err}");
            std::process::exit(1);
        });
        Arc::new(vs) as Arc<dyn memory::VectorStore>
    });

    let working_memory = skill::working_memory::new_working_memory_map();
    let mut registry = build_registry(&config.skills);
    registry.register(Box::new(skill::working_memory::MemoryWriteSkill::new(working_memory.clone())));
    registry.register(Box::new(skill::working_memory::MemoryReadSkill::new(working_memory.clone())));
    if let (Some(emb), Some(vs)) = (&embedder, &vector_store) {
        registry.register(Box::new(skill::remember::RememberSkill::new(emb.clone(), vs.clone())));
        registry.register(Box::new(skill::recall::RecallSkill::new(emb.clone(), vs.clone())));
    }
    let skill_count = registry.len();

    // Collect startup warnings for degraded states.
    let warnings = warning::new_shared_warnings();
    {
        let mut collector = warnings.write().unwrap();
        if config.models.embedding.is_none() {
            collector.add(warning::Warning {
                code: "no_embedding_model".into(),
                message: "No embedding model configured — memory features are disabled. Add a [models.embedding] section to buddy.toml.".into(),
                severity: warning::WarningSeverity::Warning,
            });
        }
        if embedder.is_some() && vector_store.is_none() {
            collector.add(warning::Warning {
                code: "no_vector_store".into(),
                message: "Vector store failed to initialize — long-term memory is unavailable.".into(),
                severity: warning::WarningSeverity::Warning,
            });
        }
        if provider_count == 1 {
            collector.add(warning::Warning {
                code: "single_chat_provider".into(),
                message: "Only one chat provider configured — no fallback available. Add additional [[models.chat.providers]] entries to buddy.toml for redundancy.".into(),
                severity: warning::WarningSeverity::Info,
            });
        }
        if let Some(vs) = &vector_store {
            if vs.needs_migration() {
                collector.add(warning::Warning {
                    code: "embedding_dimension_mismatch".into(),
                    message: "Stored embeddings don't match the current model — run POST /api/memory/migrate to re-embed.".into(),
                    severity: warning::WarningSeverity::Warning,
                });
            }
        }
    }

    // Build per-skill approval overrides from config.
    let mut approval_overrides = std::collections::HashMap::new();
    if let Some(ref cfg) = config.skills.read_file {
        if let Some(policy) = cfg.approval {
            approval_overrides.insert("read_file".to_string(), policy);
        }
    }
    if let Some(ref cfg) = config.skills.write_file {
        if let Some(policy) = cfg.approval {
            approval_overrides.insert("write_file".to_string(), policy);
        }
    }
    if let Some(ref cfg) = config.skills.fetch_url {
        if let Some(policy) = cfg.approval {
            approval_overrides.insert("fetch_url".to_string(), policy);
        }
    }

    let state = Arc::new(AppState {
        provider,
        registry,
        store,
        embedder: embedder.clone(),
        vector_store,
        working_memory,
        memory_config: config.memory.clone(),
        warnings,
        pending_approvals: new_pending_approvals(),
        conversation_approvals: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        approval_overrides,
        approval_timeout: std::time::Duration::from_secs(60),
        config: std::sync::RwLock::new(config),
        config_path,
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
