//! Application state container.
//!
//! `AppState` is the central runtime state shared across all request handlers.
//! It holds providers, skills, embedders, stores, and config. Despite
//! originating in the HTTP API layer, it has no web-framework dependencies —
//! any consumer (web server, CLI tool, bot) can construct and use it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::sync::{oneshot, Mutex};

use crate::config::{ApprovalPolicy, Config};
use crate::embedding::Embedder;
use crate::memory::VectorStore;
use crate::provider::{AnyProvider, ProviderChain};
use crate::reload;
use crate::skill::SkillRegistry;
use crate::skill::working_memory::WorkingMemoryMap;
use crate::store::Store;
use crate::warning::SharedWarnings;

/// Pending approval requests awaiting user response.
pub type PendingApprovals = Arc<Mutex<HashMap<String, oneshot::Sender<bool>>>>;

/// Skills already approved once per conversation (`once` policy).
pub type ConversationApprovals = Arc<Mutex<HashMap<String, HashSet<String>>>>;

/// Create a new empty `PendingApprovals` map.
pub fn new_pending_approvals() -> PendingApprovals {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Shared application state.
///
/// Fields wrapped in `ArcSwap` are hot-reloadable: they can be atomically
/// replaced when the configuration changes without interrupting in-flight
/// requests. Handlers call `.load()` to get a snapshot for the duration of
/// the request.
/// Handle for a managed child process (e.g. buddy-telegram).
pub type ChildProcessHandle = Arc<std::sync::Mutex<Option<std::process::Child>>>;

/// Create a new empty child process handle.
pub fn new_child_process_handle() -> ChildProcessHandle {
    Arc::new(std::sync::Mutex::new(None))
}

pub struct AppState<P> {
    pub provider: arc_swap::ArcSwap<P>,
    pub registry: arc_swap::ArcSwap<SkillRegistry>,
    pub store: Store,
    pub embedder: arc_swap::ArcSwap<Option<Arc<dyn Embedder>>>,
    pub vector_store: arc_swap::ArcSwap<Option<Arc<dyn VectorStore>>>,
    pub working_memory: WorkingMemoryMap,
    pub memory_config: arc_swap::ArcSwap<crate::config::MemoryConfig>,
    pub warnings: SharedWarnings,
    pub pending_approvals: PendingApprovals,
    pub conversation_approvals: ConversationApprovals,
    pub approval_overrides: arc_swap::ArcSwap<HashMap<String, ApprovalPolicy>>,
    pub approval_timeout: Duration,
    pub config: RwLock<Config>,
    pub config_path: PathBuf,
    /// Optional callback invoked after a successful config write to hot-reload
    /// runtime components. Set to `Some` in production (via `reload::reload_from_config`),
    /// left as `None` in tests that don't need reload behavior.
    pub on_config_change: Option<Box<dyn Fn(&Self) -> Result<(), String> + Send + Sync>>,
    /// Managed child process for buddy-telegram.
    pub telegram_process: ChildProcessHandle,
}

impl AppState<ProviderChain<AnyProvider>> {
    /// Construct a new `AppState` from a parsed config and config file path.
    ///
    /// Initialises all runtime components (provider chain, embedder, vector
    /// store, skill registry, warnings) using the `reload::build_*` functions.
    /// The `on_config_change` callback is left as `None`; callers that need
    /// hot-reload should set it before sharing the state.
    pub fn new(config: Config, config_path: &Path) -> Result<Self, String> {
        let db_path = &config.storage.database;
        let store = Store::open(Path::new(db_path))
            .map_err(|e| format!("failed to initialize database: {e}"))?;

        let working_memory = crate::skill::working_memory::new_working_memory_map();

        let provider = reload::build_provider_chain(&config)
            .map_err(|e| e.to_string())?;

        let embedder = reload::build_embedder(&config)
            .map_err(|e| e.to_string())?;

        let vector_store = reload::build_vector_store(&embedder)
            .map_err(|e| e.to_string())?;

        let registry = reload::build_skill_registry(
            &config,
            working_memory.clone(),
            &embedder,
            &vector_store,
        );

        let approval_overrides = reload::build_approval_overrides(&config);

        let provider_count = provider.len();
        let warnings = crate::warning::new_shared_warnings();
        reload::refresh_warnings(&warnings, provider_count, &embedder, &vector_store);

        Ok(Self {
            provider: arc_swap::ArcSwap::from_pointee(provider),
            registry: arc_swap::ArcSwap::from_pointee(registry),
            store,
            embedder: arc_swap::ArcSwap::from_pointee(embedder),
            vector_store: arc_swap::ArcSwap::from_pointee(vector_store),
            working_memory,
            memory_config: arc_swap::ArcSwap::from_pointee(config.memory.clone()),
            warnings,
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(approval_overrides),
            approval_timeout: Duration::from_secs(60),
            config: RwLock::new(config),
            config_path: config_path.to_path_buf(),
            on_config_change: None,
            telegram_process: new_child_process_handle(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appstate_new_with_test_config() {
        let config = Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"

[storage]
database = ":memory:"
"#,
        )
        .unwrap();

        let state = AppState::new(config, Path::new("/tmp/buddy-test-053.toml"));
        assert!(state.is_ok(), "AppState::new should succeed: {:?}", state.err());

        let state = state.unwrap();
        let cfg = state.config.read().unwrap();
        assert_eq!(cfg.models.chat.providers[0].model, "test-model");
    }
}
