use std::path::{Path, PathBuf};
use std::process;

use buddy_core::config::Config;
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};

const VALID_SECTIONS: &[&str] = &[
    "server",
    "models",
    "chat",
    "skills",
    "storage",
    "memory",
    "auth",
    "interfaces",
];

#[derive(Parser)]
#[command(name = "buddy-cli")]
struct Cli {
    /// Path to the config file
    #[arg(long, default_value = "buddy.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// View and modify configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print SHA-256 hash for a token
    HashToken {
        /// The plaintext token to hash
        token: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Pretty-print the config as TOML
    Show {
        /// Optional section name to display
        section: Option<String>,
    },
    /// Get a config value by dot notation
    Get {
        /// Dot-notation key (e.g., chat.system_prompt)
        key: String,
    },
    /// Set a scalar config value
    Set {
        /// Dot-notation key (e.g., chat.system_prompt)
        key: String,
        /// The value to set
        value: String,
    },
    /// Validate the config file
    Validate,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Command::Config { action } => match action {
            ConfigAction::Show { section } => {
                show_config(&cli.config, section.as_deref())
            }
            ConfigAction::Get { key } => get_value(&cli.config, &key),
            ConfigAction::Set { key, value } => set_value(&cli.config, &key, &value),
            ConfigAction::Validate => validate_config(&cli.config),
        },
        Command::HashToken { token } => Ok(hash_token_value(&token)),
    };

    match result {
        Ok(output) => print!("{output}"),
        Err(msg) => {
            eprint!("{msg}");
            process::exit(1);
        }
    }
}

fn read_config_file(config_path: &Path) -> Result<String, String> {
    if !config_path.exists() {
        return Err(format!(
            "Config file not found: {}. Use --config to specify the path.",
            config_path.display()
        ));
    }
    std::fs::read_to_string(config_path)
        .map_err(|e| format!("Config error: {e}"))
}

fn show_config(config_path: &Path, section: Option<&str>) -> Result<String, String> {
    let contents = read_config_file(config_path)?;
    let config = Config::parse(&contents).map_err(|e| format!("Config error: {e}"))?;

    match section {
        None => Ok(config.to_toml_string()),
        Some(name) => {
            if !VALID_SECTIONS.contains(&name) {
                return Err(format!(
                    "Unknown section '{name}'. Valid sections: server, models, chat, skills, storage, memory, auth, interfaces"
                ));
            }
            // Serialize the full config to a TOML Value, then extract the section
            let full_toml: toml::Value = toml::de::from_str(&config.to_toml_string())
                .map_err(|e| format!("Config error: {e}"))?;
            match full_toml.get(name) {
                Some(section_value) => {
                    // Wrap in a table with the section name for proper TOML output
                    let mut wrapper = toml::map::Map::new();
                    wrapper.insert(name.to_string(), section_value.clone());
                    Ok(toml::to_string_pretty(&wrapper)
                        .map_err(|e| format!("Config error: {e}"))?)
                }
                None => {
                    // Section exists in schema but is empty/default
                    let mut wrapper = toml::map::Map::new();
                    wrapper.insert(name.to_string(), toml::Value::Table(toml::map::Map::new()));
                    Ok(toml::to_string_pretty(&wrapper)
                        .map_err(|e| format!("Config error: {e}"))?)
                }
            }
        }
    }
}

fn get_value(config_path: &Path, key: &str) -> Result<String, String> {
    let contents = read_config_file(config_path)?;
    // Parse as raw TOML Value to support arbitrary dot notation
    let root: toml::Value = toml::de::from_str(&contents)
        .map_err(|e| format!("Config error: {e}"))?;

    let value = navigate_value(&root, key)
        .ok_or_else(|| format!("Key '{key}' not found"))?;

    match value {
        toml::Value::String(s) => Ok(format!("{s}\n")),
        toml::Value::Integer(n) => Ok(format!("{n}\n")),
        toml::Value::Float(f) => Ok(format!("{f}\n")),
        toml::Value::Boolean(b) => Ok(format!("{b}\n")),
        toml::Value::Datetime(d) => Ok(format!("{d}\n")),
        toml::Value::Array(_) | toml::Value::Table(_) => {
            Ok(toml::to_string_pretty(value).map_err(|e| format!("Config error: {e}"))?)
        }
    }
}

fn set_value(config_path: &Path, key: &str, value: &str) -> Result<String, String> {
    let contents = read_config_file(config_path)?;
    let mut root: toml::Value = toml::de::from_str(&contents)
        .map_err(|e| format!("Config error: {e}"))?;

    // Parse the value string into an appropriate TOML type
    let toml_value = parse_scalar(value);

    // Navigate to the parent and set the key
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err("Key must not be empty".to_string());
    }

    set_nested_value(&mut root, &parts, toml_value)?;

    // Serialize back to TOML
    let new_contents = toml::to_string_pretty(&root)
        .map_err(|e| format!("Config error: {e}"))?;

    // Validate via Config::parse before writing
    Config::parse(&new_contents)
        .map_err(|e| format!("Validation failed: {e}. Config was not modified."))?;

    // Write the file
    std::fs::write(config_path, &new_contents)
        .map_err(|e| format!("Failed to write config: {e}"))?;

    Ok(format!("Updated {key} = {value}\n"))
}

fn validate_config(config_path: &Path) -> Result<String, String> {
    let contents = read_config_file(config_path)?;
    Config::parse(&contents).map_err(|e| format!("Config error: {e}"))?;
    Ok("Configuration is valid.\n".to_string())
}

fn hash_token_value(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let result = hasher.finalize();
    format!("sha256:{}\n", hex::encode(result))
}

/// Navigate a TOML value tree using dot notation.
fn navigate_value<'a>(root: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    let parts: Vec<&str> = key.split('.').collect();
    let mut current = root;
    for part in &parts {
        current = current.get(part)?;
    }
    Some(current)
}

/// Parse a string into the most appropriate TOML scalar type.
fn parse_scalar(s: &str) -> toml::Value {
    // Try boolean
    if s == "true" {
        return toml::Value::Boolean(true);
    }
    if s == "false" {
        return toml::Value::Boolean(false);
    }
    // Try integer
    if let Ok(n) = s.parse::<i64>() {
        return toml::Value::Integer(n);
    }
    // Try float (only if it contains a dot, to avoid matching integers)
    if s.contains('.') {
        if let Ok(f) = s.parse::<f64>() {
            return toml::Value::Float(f);
        }
    }
    // Default to string
    toml::Value::String(s.to_string())
}

/// Set a value in a nested TOML structure given a slice of key parts.
fn set_nested_value(
    root: &mut toml::Value,
    parts: &[&str],
    value: toml::Value,
) -> Result<(), String> {
    if parts.len() == 1 {
        match root {
            toml::Value::Table(table) => {
                table.insert(parts[0].to_string(), value);
                Ok(())
            }
            _ => Err(format!("Cannot set key '{}' on non-table value", parts[0])),
        }
    } else {
        match root {
            toml::Value::Table(table) => {
                let entry = table
                    .entry(parts[0].to_string())
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
                set_nested_value(entry, &parts[1..], value)
            }
            _ => Err(format!(
                "Cannot navigate into '{}': not a table",
                parts[0]
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_config_toml() -> &'static str {
        r#"[server]
host = "127.0.0.1"
port = 3000

[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"

[chat]
system_prompt = "Test prompt"
"#
    }

    fn write_temp_config(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("buddy-cli-test-{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("buddy.toml");
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn cleanup_temp(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::remove_dir_all(parent).ok();
        }
    }

    #[test]
    fn show_config_returns_valid_toml_that_round_trips() {
        let path = write_temp_config("show-full", minimal_config_toml());
        let output = show_config(&path, None).unwrap();
        // The output should be valid TOML that parses back into a Config
        Config::parse(&output).expect("show_config output should round-trip through Config::parse");
        cleanup_temp(&path);
    }

    #[test]
    fn show_config_section_models() {
        let path = write_temp_config("show-models", minimal_config_toml());
        let output = show_config(&path, Some("models")).unwrap();
        // Should contain the models section
        assert!(output.contains("models"), "output should contain 'models': {output}");
        assert!(output.contains("test-model"), "output should contain model name: {output}");
        // Should NOT contain other top-level sections
        assert!(!output.contains("[server]"), "output should not contain [server]: {output}");
        assert!(!output.contains("[chat]"), "output should not contain [chat]: {output}");
        cleanup_temp(&path);
    }

    #[test]
    fn show_config_unknown_section_returns_error() {
        let path = write_temp_config("show-unknown", minimal_config_toml());
        let err = show_config(&path, Some("nonexistent")).unwrap_err();
        assert!(
            err.contains("Unknown section 'nonexistent'"),
            "error should mention unknown section: {err}"
        );
        assert!(
            err.contains("Valid sections:"),
            "error should list valid sections: {err}"
        );
        cleanup_temp(&path);
    }

    #[test]
    fn get_value_system_prompt() {
        let path = write_temp_config("get-prompt", minimal_config_toml());
        let output = get_value(&path, "chat.system_prompt").unwrap();
        assert_eq!(output.trim(), "Test prompt");
        cleanup_temp(&path);
    }

    #[test]
    fn get_value_server_port() {
        let path = write_temp_config("get-port", minimal_config_toml());
        let output = get_value(&path, "server.port").unwrap();
        assert_eq!(output.trim(), "3000");
        cleanup_temp(&path);
    }

    #[test]
    fn set_value_updates_system_prompt() {
        let path = write_temp_config("set-prompt", minimal_config_toml());
        let result = set_value(&path, "chat.system_prompt", "New prompt").unwrap();
        assert!(
            result.contains("Updated chat.system_prompt = New prompt"),
            "should confirm update: {result}"
        );
        // Verify the file was updated
        let new_output = get_value(&path, "chat.system_prompt").unwrap();
        assert_eq!(new_output.trim(), "New prompt");
        cleanup_temp(&path);
    }

    #[test]
    fn set_value_invalid_type_does_not_modify_file() {
        let path = write_temp_config("set-invalid", minimal_config_toml());
        let original = std::fs::read_to_string(&path).unwrap();
        let err = set_value(&path, "server.port", "abc").unwrap_err();
        assert!(
            err.contains("Validation failed"),
            "should report validation failure: {err}"
        );
        assert!(
            err.contains("Config was not modified"),
            "should say config was not modified: {err}"
        );
        // Verify file was not changed
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(original, after, "file should not be modified after validation failure");
        cleanup_temp(&path);
    }

    #[test]
    fn validate_config_valid() {
        let path = write_temp_config("validate-ok", minimal_config_toml());
        let output = validate_config(&path).unwrap();
        assert_eq!(output.trim(), "Configuration is valid.");
        cleanup_temp(&path);
    }

    #[test]
    fn validate_config_invalid_empty_providers() {
        let invalid_toml = r#"
[models.chat]
providers = []
"#;
        let path = write_temp_config("validate-bad", invalid_toml);
        let err = validate_config(&path).unwrap_err();
        assert!(
            err.contains("Config error:"),
            "should contain 'Config error:': {err}"
        );
        cleanup_temp(&path);
    }

    #[test]
    fn hash_token_value_correct() {
        let output = hash_token_value("my-secret");
        // Compute expected hash
        let mut hasher = Sha256::new();
        hasher.update(b"my-secret");
        let expected = format!("sha256:{}\n", hex::encode(hasher.finalize()));
        assert_eq!(output, expected);
    }

    #[test]
    fn show_config_nonexistent_path_returns_error() {
        let path = Path::new("/nonexistent/path.toml");
        let err = show_config(path, None).unwrap_err();
        assert!(
            err.contains("Config file not found"),
            "should contain 'Config file not found': {err}"
        );
        assert!(
            err.contains("/nonexistent/path.toml"),
            "should contain the path: {err}"
        );
    }
}
