//! Runtime component reloading for config hot-swap.
//!
//! Provides functions to rebuild provider chains, embedders, skill registries,
//! and warnings from a new `Config`, then atomically swap them into `AppState`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::config::{ApprovalPolicy, Config};
use crate::embedding;
use crate::memory;
use crate::provider::lmstudio::LmStudioProvider;
use crate::provider::openai::OpenAiProvider;
use crate::provider::{AnyProvider, ProviderChain};
use crate::skill;
use crate::warning;

/// Errors that can occur during hot-reload.
#[derive(Debug)]
pub enum ReloadError {
    InvalidConfig(String),
    EmbedderInit(String),
    VectorStoreInit(String),
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
            Self::EmbedderInit(msg) => write!(f, "embedder init failed: {msg}"),
            Self::VectorStoreInit(msg) => write!(f, "vector store init failed: {msg}"),
        }
    }
}

impl std::error::Error for ReloadError {}

/// Build a provider chain from the current config.
pub fn build_provider_chain(config: &Config) -> Result<ProviderChain<AnyProvider>, ReloadError> {
    let system_prompt = &config.chat.system_prompt;
    let mut chain_entries: Vec<(AnyProvider, String)> = Vec::new();

    for entry in &config.models.chat.providers {
        let endpoint = entry.endpoint.as_deref().ok_or_else(|| {
            ReloadError::InvalidConfig(format!(
                "endpoint is required for provider type '{}'",
                entry.provider_type
            ))
        })?;

        let api_key = entry
            .resolve_api_key()
            .map_err(ReloadError::InvalidConfig)?;

        let provider = match entry.provider_type.as_str() {
            "openai" => {
                if api_key.is_empty() {
                    return Err(ReloadError::InvalidConfig(
                        "an API key is required when type = \"openai\"".into(),
                    ));
                }
                AnyProvider::OpenAi(OpenAiProvider::new(
                    &api_key,
                    &entry.model,
                    endpoint,
                    system_prompt,
                ))
            }
            "lmstudio" => AnyProvider::LmStudio(LmStudioProvider::new(
                &entry.model,
                endpoint,
                system_prompt,
            )),
            other => {
                return Err(ReloadError::InvalidConfig(format!(
                    "unknown provider type '{other}'"
                )));
            }
        };
        chain_entries.push((provider, entry.model.clone()));
    }

    Ok(ProviderChain::new(chain_entries))
}

/// Build the embedder from config.
///
/// The local embedder (fastembed, all-MiniLM-L6-v2) is always available as a
/// built-in default. If the config specifies an embedding provider with
/// `type = "local"`, the same local embedder is used. Non-local provider
/// types are not yet supported and are ignored.
///
/// Returns `Some` in all current configurations. The `Option` is retained for
/// forward-compatibility with cases where an external provider is configured
/// but unavailable.
pub fn build_embedder(
    config: &Config,
) -> Result<Option<Arc<dyn embedding::Embedder>>, ReloadError> {
    // If an embedding section exists, look for a configured provider.
    // Currently only "local" is supported; external types will be added
    // in a future version.
    let has_local = config
        .models
        .embedding
        .as_ref()
        .map(|slot| slot.providers.iter().any(|e| e.provider_type == "local"))
        .unwrap_or(false);

    // Use the local embedder as default when no embedding section exists
    // or when "local" is explicitly configured.
    let needs_local = has_local
        || config.models.embedding.is_none()
        || config
            .models
            .embedding
            .as_ref()
            .is_some_and(|slot| slot.providers.is_empty());

    if needs_local {
        let embedder = embedding::local::LocalEmbedder::new()
            .map_err(|e| ReloadError::EmbedderInit(e.to_string()))?;
        return Ok(Some(Arc::new(embedder) as Arc<dyn embedding::Embedder>));
    }

    Ok(None)
}

/// Build the optional vector store when an embedder is available.
pub fn build_vector_store(
    embedder: &Option<Arc<dyn embedding::Embedder>>,
) -> Result<Option<Arc<dyn memory::VectorStore>>, ReloadError> {
    let result = embedder
        .as_ref()
        .map(|e| {
            memory::sqlite::SqliteVectorStore::open(
                Path::new("memory.db"),
                e.model_name(),
                e.dimensions(),
            )
            .map(|vs| Arc::new(vs) as Arc<dyn memory::VectorStore>)
        })
        .transpose()
        .map_err(|e| ReloadError::VectorStoreInit(e.to_string()))?;
    Ok(result)
}

/// Build the skill registry from config, including memory skills.
pub fn build_skill_registry(
    config: &Config,
    working_memory: skill::working_memory::WorkingMemoryMap,
    embedder: &Option<Arc<dyn embedding::Embedder>>,
    vector_store: &Option<Arc<dyn memory::VectorStore>>,
) -> skill::SkillRegistry {
    let mut registry = skill::build_registry(&config.skills);
    registry.register(Box::new(skill::working_memory::MemoryWriteSkill::new(
        working_memory.clone(),
    )));
    registry.register(Box::new(skill::working_memory::MemoryReadSkill::new(
        working_memory,
    )));
    if let (Some(emb), Some(vs)) = (embedder, vector_store) {
        registry.register(Box::new(skill::remember::RememberSkill::new(
            emb.clone(),
            vs.clone(),
        )));
        registry.register(Box::new(skill::recall::RecallSkill::new(
            emb.clone(),
            vs.clone(),
        )));
    }
    registry
}

/// Extract per-skill approval overrides from config.
pub fn build_approval_overrides(
    config: &Config,
) -> HashMap<String, ApprovalPolicy> {
    let mut map = HashMap::new();
    if let Some(ref cfg) = config.skills.read_file {
        if let Some(policy) = cfg.approval {
            map.insert("read_file".to_string(), policy);
        }
    }
    if let Some(ref cfg) = config.skills.write_file {
        if let Some(policy) = cfg.approval {
            map.insert("write_file".to_string(), policy);
        }
    }
    if let Some(ref cfg) = config.skills.fetch_url {
        if let Some(policy) = cfg.approval {
            map.insert("fetch_url".to_string(), policy);
        }
    }
    map
}

/// Refresh warnings to reflect the current state of hot-reloadable components.
///
/// Clears all config-related warnings and re-runs the same checks that happen
/// at startup.
pub fn refresh_warnings(
    warnings: &warning::SharedWarnings,
    provider_count: usize,
    embedder: &Option<Arc<dyn embedding::Embedder>>,
    vector_store: &Option<Arc<dyn memory::VectorStore>>,
) {
    let mut collector = warnings.write().unwrap();

    // Clear stale config-related warnings.
    collector.clear("no_vector_store");
    collector.clear("single_chat_provider");
    collector.clear("embedding_dimension_mismatch");

    if vector_store.is_none() {
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

    if let Some(vs) = vector_store {
        if vs.needs_migration() {
            collector.add(warning::Warning {
                code: "embedding_dimension_mismatch".into(),
                message: "Stored embeddings don't match the current model — run POST /api/memory/migrate to re-embed.".into(),
                severity: warning::WarningSeverity::Warning,
            });
        }
    }
}

/// Perform a full hot-reload: rebuild all runtime components from config and
/// swap them into `AppState`.
///
/// On error, the existing state is unchanged — the old components remain active.
pub fn reload_from_config(
    config: &Config,
    state: &crate::api::AppState<ProviderChain<AnyProvider>>,
) -> Result<(), ReloadError> {
    let provider = build_provider_chain(config)?;
    let embedder = build_embedder(config)?;
    let vector_store = build_vector_store(&embedder)?;
    let registry = build_skill_registry(
        config,
        state.working_memory.clone(),
        &embedder,
        &vector_store,
    );
    let memory_config = config.memory.clone();
    let approval_overrides = build_approval_overrides(config);
    let provider_count = provider.len();

    // Atomically swap all hot-reloadable fields.
    state.provider.store(Arc::new(provider));
    state.registry.store(Arc::new(registry));
    state.embedder.store(Arc::new(embedder.clone()));
    state.vector_store.store(Arc::new(vector_store.clone()));
    state.memory_config.store(Arc::new(memory_config));
    state.approval_overrides.store(Arc::new(approval_overrides));

    refresh_warnings(&state.warnings, provider_count, &embedder, &vector_store);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn lmstudio_config() -> Config {
        Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap()
    }

    fn two_provider_config() -> Config {
        Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "model-a"
endpoint = "http://localhost:1234/v1"

[[models.chat.providers]]
type = "lmstudio"
model = "model-b"
endpoint = "http://localhost:5678/v1"
"#,
        )
        .unwrap()
    }

    #[test]
    fn build_provider_chain_single() {
        let config = lmstudio_config();
        let chain = build_provider_chain(&config).unwrap();
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn build_provider_chain_two() {
        let config = two_provider_config();
        let chain = build_provider_chain(&config).unwrap();
        assert_eq!(chain.len(), 2);
    }

    #[test]
    fn build_embedder_defaults_to_local_when_not_configured() {
        let config = lmstudio_config();
        let embedder = build_embedder(&config).unwrap();
        assert!(embedder.is_some(), "local embedder should be active by default");
        assert_eq!(embedder.unwrap().model_name(), "all-MiniLM-L6-v2");
    }

    #[test]
    fn build_approval_overrides_empty_by_default() {
        let config = lmstudio_config();
        let overrides = build_approval_overrides(&config);
        assert!(overrides.is_empty());
    }

    #[test]
    fn build_approval_overrides_from_skills() {
        let config = Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"

[skills.read_file]
allowed_directories = ["/tmp"]
approval = "trust"
"#,
        )
        .unwrap();
        let overrides = build_approval_overrides(&config);
        assert_eq!(overrides.get("read_file"), Some(&ApprovalPolicy::Trust));
    }

    #[test]
    fn refresh_warnings_no_embedder_adds_vector_store_warning() {
        let warnings = warning::new_shared_warnings();
        refresh_warnings(&warnings, 2, &None, &None);
        let collector = warnings.read().unwrap();
        let list = collector.list();
        assert!(list.iter().any(|w| w.code == "no_vector_store"));
        assert!(!list.iter().any(|w| w.code == "single_chat_provider"));
    }

    #[test]
    fn refresh_warnings_single_provider() {
        let warnings = warning::new_shared_warnings();
        refresh_warnings(&warnings, 1, &None, &None);
        let collector = warnings.read().unwrap();
        let list = collector.list();
        assert!(list.iter().any(|w| w.code == "single_chat_provider"));
    }

    #[test]
    fn refresh_warnings_clears_stale() {
        let warnings = warning::new_shared_warnings();
        {
            let mut c = warnings.write().unwrap();
            c.add(warning::Warning {
                code: "no_vector_store".into(),
                message: "stale".into(),
                severity: warning::WarningSeverity::Warning,
            });
        }
        // Refresh with no embedder/store: the stale warning should be replaced,
        // not duplicated.
        refresh_warnings(&warnings, 2, &None, &None);
        let collector = warnings.read().unwrap();
        let list = collector.list();
        let count = list.iter().filter(|w| w.code == "no_vector_store").count();
        assert_eq!(count, 1, "should not duplicate warnings after refresh");
    }
}
