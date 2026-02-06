use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3000;
const DEFAULT_CONFIG_PATH: &str = "buddy.toml";
const DEFAULT_DATABASE: &str = "buddy.db";

#[derive(Parser)]
#[command(name = "buddy-server")]
struct Cli {
    /// Path to the configuration file
    #[arg(long = "config", default_value = DEFAULT_CONFIG_PATH)]
    config: PathBuf,
}

#[derive(Debug, Deserialize, PartialEq)]
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
}

#[derive(Debug, Deserialize, PartialEq)]
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

#[derive(Debug, Deserialize, PartialEq)]
pub struct ModelsConfig {
    pub chat: ModelSlot,
    pub embedding: Option<ModelSlot>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ModelSlot {
    pub providers: Vec<ProviderEntry>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct ProviderEntry {
    #[serde(default = "default_provider_type", rename = "type")]
    pub provider_type: String,
    pub model: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

impl ProviderEntry {
    pub fn resolve_api_key(&self) -> Result<String, String> {
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

#[derive(Debug, Deserialize, PartialEq)]
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

#[derive(Debug, Deserialize, PartialEq)]
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

#[derive(Debug, Deserialize, PartialEq, Default)]
pub struct SkillsConfig {
    pub read_file: Option<ReadFileConfig>,
    pub write_file: Option<WriteFileConfig>,
    pub fetch_url: Option<FetchUrlConfig>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct ReadFileConfig {
    pub allowed_directories: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct WriteFileConfig {
    pub allowed_directories: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct FetchUrlConfig {
    pub allowed_domains: Vec<String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let cli = Cli::parse();
        Self::from_file(&cli.config)
    }

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
    fn system_prompt_default_when_chat_section_omitted() {
        let config = Config::parse(minimal_chat_toml()).unwrap();
        assert_eq!(
            config.chat.system_prompt,
            "You are a helpful, friendly AI assistant."
        );
    }
}
