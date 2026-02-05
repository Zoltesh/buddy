use clap::Parser;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 3000;
const DEFAULT_CONFIG_PATH: &str = "buddy.toml";

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
    pub provider: ProviderConfig,
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
pub struct ProviderConfig {
    pub api_key: String,
    pub model: String,
    pub endpoint: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
}

fn default_system_prompt() -> String {
    DEFAULT_SYSTEM_PROMPT.to_string()
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
        toml::from_str(contents).map_err(|e| format!("invalid config: {e}"))
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_valid_config() {
        let toml = r#"
[provider]
api_key = "sk-test-123"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.provider.api_key, "sk-test-123");
        assert_eq!(config.provider.model, "gpt-4");
        assert_eq!(config.provider.endpoint, "https://api.openai.com/v1");
    }

    #[test]
    fn missing_api_key_produces_clear_error() {
        let toml = r#"
[provider]
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
"#;
        let err = Config::parse(toml).unwrap_err();
        assert!(err.contains("api_key"), "error should mention api_key: {err}");
    }

    #[test]
    fn missing_server_section_uses_defaults() {
        let toml = r#"
[provider]
api_key = "sk-test-123"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.bind_address(), "127.0.0.1:3000");
    }

    #[test]
    fn config_flag_reads_specified_file() {
        let dir = std::env::temp_dir().join("buddy-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("custom.toml");
        std::fs::write(
            &path,
            r#"
[provider]
api_key = "sk-custom"
model = "claude-3"
endpoint = "https://api.anthropic.com/v1"
"#,
        )
        .unwrap();

        let config = Config::from_file(&path).unwrap();
        assert_eq!(config.provider.api_key, "sk-custom");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_provider_section_produces_error() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080
"#;
        let err = Config::parse(toml).unwrap_err();
        assert!(
            err.contains("provider"),
            "error should mention provider: {err}"
        );
    }

    #[test]
    fn custom_server_values_override_defaults() {
        let toml = r#"
[server]
host = "0.0.0.0"
port = 8080

[provider]
api_key = "sk-test"
model = "gpt-4"
endpoint = "https://api.openai.com/v1"
"#;
        let config = Config::parse(toml).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
    }
}
