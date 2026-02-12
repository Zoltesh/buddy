use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3000;
const DEFAULT_DATABASE: &str = "buddy.db";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    pub models: ModelsConfig,
    #[serde(default)]
    pub chat: ChatConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub interfaces: InterfacesConfig,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful, friendly AI assistant.";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ModelsConfig {
    pub chat: ModelSlot,
    pub embedding: Option<ModelSlot>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ModelSlot {
    pub providers: Vec<ProviderEntry>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ProviderEntry {
    #[serde(default = "default_provider_type", rename = "type")]
    pub provider_type: String,
    pub model: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

impl ProviderEntry {
    pub fn resolve_api_key(&self) -> Result<String, String> {
        if let Some(ref key) = self.api_key {
            if !key.is_empty() {
                return Ok(key.clone());
            }
        }
        match &self.api_key_env {
            Some(var_name) => std::env::var(var_name).map_err(|_| {
                format!(
                    "environment variable '{var_name}' is not set (required by api_key_env)"
                )
            }),
            None => Ok(String::new()),
        }
    }
}

fn default_provider_type() -> String {
    "openai".to_string()
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ChatConfig {
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            system_prompt: default_system_prompt(),
        }
    }
}

fn default_system_prompt() -> String {
    DEFAULT_SYSTEM_PROMPT.to_string()
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct StorageConfig {
    #[serde(default = "default_database")]
    pub database: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database: default_database(),
        }
    }
}

fn default_database() -> String {
    DEFAULT_DATABASE.to_string()
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Default)]
pub struct SkillsConfig {
    pub read_file: Option<ReadFileConfig>,
    pub write_file: Option<WriteFileConfig>,
    pub fetch_url: Option<FetchUrlConfig>,
}

/// Per-skill approval policy for mutating or network skills.
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalPolicy {
    /// Ask every time (default for Mutating and Network).
    Always,
    /// Ask once per conversation, then auto-approve for that skill.
    Once,
    /// Never ask (auto-approve).
    Trust,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ReadFileConfig {
    pub allowed_directories: Vec<String>,
    #[serde(default)]
    pub approval: Option<ApprovalPolicy>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct WriteFileConfig {
    pub allowed_directories: Vec<String>,
    #[serde(default)]
    pub approval: Option<ApprovalPolicy>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct FetchUrlConfig {
    pub allowed_domains: Vec<String>,
    #[serde(default)]
    pub approval: Option<ApprovalPolicy>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct MemoryConfig {
    #[serde(default = "default_auto_retrieve")]
    pub auto_retrieve: bool,
    #[serde(default = "default_auto_retrieve_limit")]
    pub auto_retrieve_limit: usize,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f32,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            auto_retrieve: default_auto_retrieve(),
            auto_retrieve_limit: default_auto_retrieve_limit(),
            similarity_threshold: default_similarity_threshold(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Default)]
pub struct AuthConfig {
    pub token_hash: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Default)]
pub struct InterfacesConfig {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,
}

const DEFAULT_BOT_TOKEN_ENV: &str = "TELEGRAM_BOT_TOKEN";

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_bot_token_env")]
    pub bot_token_env: String,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bot_token_env: default_bot_token_env(),
        }
    }
}

fn default_bot_token_env() -> String {
    DEFAULT_BOT_TOKEN_ENV.to_string()
}

const DEFAULT_WHATSAPP_API_TOKEN_ENV: &str = "WHATSAPP_API_TOKEN";
const DEFAULT_WHATSAPP_APP_SECRET_ENV: &str = "WHATSAPP_APP_SECRET";
const DEFAULT_WHATSAPP_WEBHOOK_PORT: u16 = 8444;

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct WhatsAppConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_whatsapp_api_token_env")]
    pub api_token_env: String,
    #[serde(default = "default_whatsapp_app_secret_env")]
    pub app_secret_env: String,
    #[serde(default)]
    pub phone_number_id: String,
    #[serde(default)]
    pub verify_token: String,
    #[serde(default = "default_whatsapp_webhook_port")]
    pub webhook_port: u16,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_token_env: default_whatsapp_api_token_env(),
            app_secret_env: default_whatsapp_app_secret_env(),
            phone_number_id: String::new(),
            verify_token: String::new(),
            webhook_port: DEFAULT_WHATSAPP_WEBHOOK_PORT,
        }
    }
}

fn default_whatsapp_api_token_env() -> String {
    DEFAULT_WHATSAPP_API_TOKEN_ENV.to_string()
}

fn default_whatsapp_app_secret_env() -> String {
    DEFAULT_WHATSAPP_APP_SECRET_ENV.to_string()
}

fn default_whatsapp_webhook_port() -> u16 {
    DEFAULT_WHATSAPP_WEBHOOK_PORT
}

fn default_auto_retrieve() -> bool {
    true
}

fn default_auto_retrieve_limit() -> usize {
    3
}

fn default_similarity_threshold() -> f32 {
    0.5
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read config file '{}': {e}", path.display()))?;
        Self::parse(&contents)
    }

    pub fn parse(contents: &str) -> Result<Self, String> {
        let config: Config = toml::from_str(contents).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("missing field `models`") || msg.contains("missing field `chat`") {
                return "invalid config: [models.chat] section is required".to_string();
            }
            format!("invalid config: {msg}")
        })?;
        if config.models.chat.providers.is_empty() {
            return Err(
                "invalid config: models.chat.providers must not be empty".to_string(),
            );
        }
        Ok(config)
    }

    pub fn to_toml_string(&self) -> String {
        toml::to_string_pretty(self).expect("Config should always be serializable to TOML")
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_chat_toml() -> &'static str {
        r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"
"#
    }

    #[test]
    fn parse_minimal_valid_config() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        let primary = &config.models.chat.providers[0];
        assert_eq!(primary.provider_type, "lmstudio");
        assert_eq!(primary.model, "deepseek-coder");
        assert_eq!(
            primary.endpoint.as_deref(),
            Some("http://localhost:1234/v1")
        );
    }

    #[test]
    fn default_provider_type_is_openai() {
        let toml = r#"
[[models.chat.providers]]
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "openai");
    }

    #[test]
    fn explicit_provider_type_is_parsed() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "lmstudio");
    }

    #[test]
    fn missing_server_section_uses_defaults() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.bind_address(), "127.0.0.1:3000");
    }

    #[test]
    fn config_flag_reads_specified_file() {
        let dir = std::env::temp_dir().join("buddy-config-test-018");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("custom.toml");
        std::fs::write(&path, minimal_chat_toml()).unwrap();

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.models.chat.providers[0].model, "deepseek-coder");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_models_chat_produces_error() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080
"#;
        let err = Config::parse(toml).unwrap_err();
        assert!(
            err.contains("models.chat"),
            "error should mention models.chat: {err}"
        );
    }

    #[test]
    fn custom_server_values_override_defaults() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
    }

    #[test]
    fn no_skills_section_uses_defaults() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert!(config.skills.read_file.is_none());
        assert!(config.skills.write_file.is_none());
        assert!(config.skills.fetch_url.is_none());
    }

    #[test]
    fn no_storage_section_uses_default_database_path() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert_eq!(config.storage.database, "buddy.db");
    }

    #[test]
    fn custom_storage_database_path() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[storage]
database = "/var/data/buddy.db"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.storage.database, "/var/data/buddy.db");
    }

    #[test]
    fn skills_read_file_only() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[skills.read_file]
allowed_directories = ["/home/user/documents"]
"#;
        let config = Config::parse(toml).unwrap();
        assert!(config.skills.read_file.is_some());
        assert_eq!(
            config.skills.read_file.unwrap().allowed_directories,
            vec!["/home/user/documents"]
        );
        assert!(config.skills.write_file.is_none());
        assert!(config.skills.fetch_url.is_none());
    }

    #[test]
    fn full_skills_config_parses() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[skills.read_file]
allowed_directories = ["/home/user/docs", "/tmp/shared"]

[skills.write_file]
allowed_directories = ["/home/user/sandbox"]

[skills.fetch_url]
allowed_domains = ["example.com", "api.github.com"]
"#;
        let config = Config::parse(toml).unwrap();

        let rf = config.skills.read_file.unwrap();
        assert_eq!(rf.allowed_directories, vec!["/home/user/docs", "/tmp/shared"]);

        let wf = config.skills.write_file.unwrap();
        assert_eq!(wf.allowed_directories, vec!["/home/user/sandbox"]);

        let fu = config.skills.fetch_url.unwrap();
        assert_eq!(fu.allowed_domains, vec!["example.com", "api.github.com"]);
    }

    #[test]
    fn chat_with_two_providers_stored_in_order() {
        let toml = r#"
[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers.len(), 2);
        assert_eq!(config.models.chat.providers[0].provider_type, "openai");
        assert_eq!(config.models.chat.providers[0].model, "gpt-4");
        assert_eq!(config.models.chat.providers[1].provider_type, "lmstudio");
        assert_eq!(config.models.chat.providers[1].model, "deepseek-coder");
    }

    #[test]
    fn embedding_with_local_provider_parses() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"
"#;
        let config = Config::parse(toml).unwrap();
        let embedding = config.models.embedding.as_ref().unwrap();
        assert_eq!(embedding.providers.len(), 1);
        assert_eq!(embedding.providers[0].provider_type, "local");
        assert_eq!(embedding.providers[0].model, "all-minilm");
        assert!(embedding.providers[0].endpoint.is_none());
    }

    #[test]
    fn no_embedding_section_is_none() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert!(config.models.embedding.is_none());
    }

    #[test]
    fn empty_providers_list_produces_error() {
        let toml = r#"
[models.chat]
providers = []
"#;
        let err = Config::parse(toml).unwrap_err();
        assert!(
            err.contains("providers") && err.contains("empty"),
            "error should mention empty providers: {err}"
        );
    }

    #[test]
    fn api_key_env_resolves_from_environment() {
        let entry = ProviderEntry {
            provider_type: "openai".into(),
            model: "gpt-4".into(),
            endpoint: Some("https://api.openai.com/v1".into()),
            api_key: None,
            api_key_env: Some("BUDDY_TEST_API_KEY_018".into()),
        };
        // SAFETY: test-only; unique env var name avoids conflicts with other tests.
        unsafe { std::env::set_var("BUDDY_TEST_API_KEY_018", "test123") };
        let key = entry.resolve_api_key().unwrap();
        unsafe { std::env::remove_var("BUDDY_TEST_API_KEY_018") };
        assert_eq!(key, "test123");
    }

    #[test]
    fn api_key_env_unset_produces_error() {
        let entry = ProviderEntry {
            provider_type: "openai".into(),
            model: "gpt-4".into(),
            endpoint: Some("https://api.openai.com/v1".into()),
            api_key: None,
            api_key_env: Some("BUDDY_NONEXISTENT_KEY_018".into()),
        };
        unsafe { std::env::remove_var("BUDDY_NONEXISTENT_KEY_018") };
        let err = entry.resolve_api_key().unwrap_err();
        assert!(
            err.contains("BUDDY_NONEXISTENT_KEY_018"),
            "error should mention the variable name: {err}"
        );
    }

    #[test]
    fn direct_api_key_resolves() {
        let entry = ProviderEntry {
            provider_type: "openai".into(),
            model: "gpt-4".into(),
            endpoint: Some("https://api.openai.com/v1".into()),
            api_key: Some("sk-direct-key".into()),
            api_key_env: None,
        };
        assert_eq!(entry.resolve_api_key().unwrap(), "sk-direct-key");
    }

    #[test]
    fn direct_api_key_takes_priority_over_env_var() {
        let entry = ProviderEntry {
            provider_type: "openai".into(),
            model: "gpt-4".into(),
            endpoint: Some("https://api.openai.com/v1".into()),
            api_key: Some("sk-direct".into()),
            api_key_env: Some("BUDDY_TEST_PRIORITY_KEY".into()),
        };
        unsafe { std::env::set_var("BUDDY_TEST_PRIORITY_KEY", "from-env") };
        let key = entry.resolve_api_key().unwrap();
        unsafe { std::env::remove_var("BUDDY_TEST_PRIORITY_KEY") };
        assert_eq!(key, "sk-direct");
    }

    #[test]
    fn empty_direct_api_key_falls_through_to_env_var() {
        let entry = ProviderEntry {
            provider_type: "openai".into(),
            model: "gpt-4".into(),
            endpoint: Some("https://api.openai.com/v1".into()),
            api_key: Some("".into()),
            api_key_env: Some("BUDDY_TEST_FALLTHROUGH_KEY".into()),
        };
        unsafe { std::env::set_var("BUDDY_TEST_FALLTHROUGH_KEY", "env-value") };
        let key = entry.resolve_api_key().unwrap();
        unsafe { std::env::remove_var("BUDDY_TEST_FALLTHROUGH_KEY") };
        assert_eq!(key, "env-value");
    }

    #[test]
    fn memory_defaults_when_section_omitted() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert!(config.memory.auto_retrieve);
        assert_eq!(config.memory.auto_retrieve_limit, 3);
        assert!((config.memory.similarity_threshold - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn memory_config_overrides() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[memory]
auto_retrieve = false
auto_retrieve_limit = 10
similarity_threshold = 0.7
"#;
        let config = Config::parse(toml).unwrap();
        assert!(!config.memory.auto_retrieve);
        assert_eq!(config.memory.auto_retrieve_limit, 10);
        assert!((config.memory.similarity_threshold - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn to_toml_string_round_trips() {
        let toml_input = r#"
[server]
host = "0.0.0.0"
port = 8080

[[models.chat.providers]]
type = "openai"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
api_key_env = "MY_KEY"

[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[[models.embedding.providers]]
type = "local"
model = "all-minilm"

[chat]
system_prompt = "Be helpful."

[skills.read_file]
allowed_directories = ["/tmp"]

[skills.fetch_url]
allowed_domains = ["example.com"]

[memory]
auto_retrieve = false
auto_retrieve_limit = 10
similarity_threshold = 0.7
"#;
        let config = Config::parse(toml_input).unwrap();
        let serialized = config.to_toml_string();
        let reparsed = Config::parse(&serialized).unwrap();
        assert_eq!(config, reparsed);
    }

    #[test]
    fn system_prompt_default_when_chat_section_omitted() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert_eq!(
            config.chat.system_prompt,
            "You are a helpful, friendly AI assistant."
        );
    }

    // Test cases for task 046: Ollama Provider

    #[test]
    fn parse_ollama_provider_with_no_endpoint_defaults_to_localhost_11434() {
        let toml = r#"
[[models.chat.providers]]
type = "ollama"
model = "llama3"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "ollama");
        assert_eq!(config.models.chat.providers[0].model, "llama3");
        assert!(config.models.chat.providers[0].endpoint.is_none());
    }

    #[test]
    fn parse_ollama_provider_with_custom_endpoint() {
        let toml = r#"
[[models.chat.providers]]
type = "ollama"
model = "llama3"
endpoint = "http://192.168.1.100:11434"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "ollama");
        assert_eq!(config.models.chat.providers[0].model, "llama3");
        assert_eq!(
            config.models.chat.providers[0].endpoint.as_deref(),
            Some("http://192.168.1.100:11434")
        );
    }

    // Test cases for task 048: Mistral Provider

    #[test]
    fn parse_mistral_provider_with_defaults() {
        let toml = r#"
[[models.chat.providers]]
type = "mistral"
model = "mistral-large-latest"
api_key_env = "MISTRAL_API_KEY"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "mistral");
        assert_eq!(config.models.chat.providers[0].model, "mistral-large-latest");
        assert_eq!(
            config.models.chat.providers[0].api_key_env.as_deref(),
            Some("MISTRAL_API_KEY")
        );
        // endpoint is optional — will default to https://api.mistral.ai in reload.rs
        assert!(config.models.chat.providers[0].endpoint.is_none());
    }

    #[test]
    fn parse_mistral_provider_with_custom_endpoint() {
        let toml = r#"
[[models.chat.providers]]
type = "mistral"
model = "mistral-large-latest"
api_key_env = "MISTRAL_API_KEY"
endpoint = "https://custom.mistral.example"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.models.chat.providers[0].provider_type, "mistral");
        assert_eq!(config.models.chat.providers[0].model, "mistral-large-latest");
        assert_eq!(
            config.models.chat.providers[0].endpoint.as_deref(),
            Some("https://custom.mistral.example")
        );
    }

    // Test cases for task 054: Auth Config

    #[test]
    fn parse_config_with_auth_token_hash() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[auth]
token_hash = "sha256:abc123def456"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(
            config.auth.token_hash,
            Some("sha256:abc123def456".to_string())
        );
    }

    #[test]
    fn parse_config_without_auth_section() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert_eq!(config.auth.token_hash, None);
    }

    // Test cases for task 056: Telegram Config

    #[test]
    fn parse_config_with_telegram_enabled() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[interfaces.telegram]
enabled = true
bot_token_env = "TELEGRAM_BOT_TOKEN"
"#;
        let config = Config::parse(toml).unwrap();
        assert!(config.interfaces.telegram.enabled);
        assert_eq!(
            config.interfaces.telegram.bot_token_env,
            "TELEGRAM_BOT_TOKEN"
        );
    }

    #[test]
    fn parse_config_without_telegram_defaults_to_disabled() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert!(!config.interfaces.telegram.enabled);
        assert_eq!(
            config.interfaces.telegram.bot_token_env,
            "TELEGRAM_BOT_TOKEN"
        );
    }

    // Test cases for task 059: WhatsApp Config

    #[test]
    fn parse_config_with_whatsapp_all_fields() {
        let toml = r#"
[[models.chat.providers]]
type = "lmstudio"
model = "deepseek-coder"
endpoint = "http://localhost:1234/v1"

[interfaces.whatsapp]
enabled = true
api_token_env = "MY_WHATSAPP_TOKEN"
phone_number_id = "123456789"
verify_token = "my-secret-verify"
webhook_port = 9000
"#;
        let config = Config::parse(toml).unwrap();
        assert!(config.interfaces.whatsapp.enabled);
        assert_eq!(config.interfaces.whatsapp.api_token_env, "MY_WHATSAPP_TOKEN");
        assert_eq!(config.interfaces.whatsapp.phone_number_id, "123456789");
        assert_eq!(config.interfaces.whatsapp.verify_token, "my-secret-verify");
        assert_eq!(config.interfaces.whatsapp.webhook_port, 9000);
    }

    #[test]
    fn parse_config_without_whatsapp_defaults_to_disabled() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert!(!config.interfaces.whatsapp.enabled);
        assert_eq!(
            config.interfaces.whatsapp.api_token_env,
            "WHATSAPP_API_TOKEN"
        );
        assert_eq!(config.interfaces.whatsapp.phone_number_id, "");
        assert_eq!(config.interfaces.whatsapp.verify_token, "");
        assert_eq!(config.interfaces.whatsapp.webhook_port, 8444);
    }
}
