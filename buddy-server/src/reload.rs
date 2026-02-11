//! App-specific hot-reload logic for buddy-server.
//!
//! Re-exports builder functions from buddy_core::reload and provides the
//! `reload_from_config` function that updates AppState.

use std::sync::Arc;

use buddy_core::config::Config;
use buddy_core::provider::{AnyProvider, ProviderChain};
pub use buddy_core::reload::{
    build_approval_overrides, build_embedder, build_provider_chain, build_skill_registry,
    build_vector_store, refresh_warnings, ReloadError,
};

/// Perform a full hot-reload: rebuild all runtime components from config and
/// swap them into `AppState`.
///
/// On error, the existing state is unchanged — the old components remain active.
pub fn reload_from_config(
    config: &Config,
    state: &buddy_core::state::AppState<ProviderChain<AnyProvider>>,
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
    use buddy_core::config::Config;
    use buddy_core::skill;
    use buddy_core::state::AppState;
    use buddy_core::store::Store;
    use buddy_core::warning;

    #[test]
    fn reload_from_config_activates_local_embedder_when_external_removed() {
        // Start with a config that has an external provider (local type)
        let config_with_external = Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"

[[models.embedding.providers]]
type = "local"
model = "all-MiniLM-L6-v2"
"#,
        )
        .unwrap();

        // Create AppState with the initial config
        let tmp = std::env::temp_dir().join("buddy_test_reload_embedder");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let db_path = tmp.join("test.db");

        let store = Store::open(&db_path).unwrap();
        let provider = build_provider_chain(&config_with_external).unwrap();
        let embedder = build_embedder(&config_with_external).unwrap();
        let vector_store = build_vector_store(&embedder).unwrap();
        let working_memory = skill::working_memory::new_working_memory_map();
        let registry = build_skill_registry(
            &config_with_external,
            working_memory.clone(),
            &embedder,
            &vector_store,
        );
        let approval_overrides = build_approval_overrides(&config_with_external);
        let warnings = warning::new_shared_warnings();

        let state = AppState {
            provider: arc_swap::ArcSwap::from_pointee(provider),
            registry: arc_swap::ArcSwap::from_pointee(registry),
            store,
            embedder: arc_swap::ArcSwap::from_pointee(embedder),
            vector_store: arc_swap::ArcSwap::from_pointee(vector_store),
            working_memory,
            memory_config: arc_swap::ArcSwap::from_pointee(config_with_external.memory.clone()),
            warnings,
            pending_approvals: buddy_core::state::new_pending_approvals(),
            conversation_approvals: Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            approval_overrides: arc_swap::ArcSwap::from_pointee(approval_overrides),
            approval_timeout: std::time::Duration::from_secs(60),
            config: std::sync::RwLock::new(config_with_external),
            config_path: tmp.join("buddy.toml"),
            on_config_change: None,
        };

        // Config without external embedding providers (should activate local)
        let config_without_external = Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap();

        reload_from_config(&config_without_external, &state).unwrap();

        // Assert the embedder is now a LocalEmbedder
        let embedder = state.embedder.load();
        assert!(embedder.is_some(), "embedder should be Some after reload");
        let embedder = embedder.as_ref().as_ref().unwrap();
        assert_eq!(
            embedder.model_name(),
            "all-MiniLM-L6-v2",
            "should be local embedder after reload"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
