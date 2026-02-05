pub mod fetch_url;
pub mod read_file;
pub mod write_file;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::config::SkillsConfig;

/// Errors that can occur when executing a skill.
#[derive(Debug)]
pub enum SkillError {
    /// The input provided to the skill was invalid.
    InvalidInput(String),
    /// The requested operation is not permitted.
    Forbidden(String),
    /// The skill execution failed for an operational reason.
    ExecutionFailed(String),
}

impl std::fmt::Display for SkillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::Forbidden(msg) => write!(f, "forbidden: {msg}"),
            Self::ExecutionFailed(msg) => write!(f, "execution failed: {msg}"),
        }
    }
}

impl std::error::Error for SkillError {}

/// A callable tool capability that can be exposed to LLM providers.
///
/// Implementors must be `Send + Sync` so they can be stored in the registry
/// and invoked from async handlers.
pub trait Skill: Send + Sync {
    /// Unique name of the skill (used as the function name in tool calls).
    fn name(&self) -> &str;

    /// Human-readable description of what the skill does.
    fn description(&self) -> &str;

    /// JSON Schema describing the expected input.
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the skill with the given input and return a result.
    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>>;
}

/// Registry of all available skills.
///
/// Skills are registered at startup and looked up by name when the LLM
/// requests a tool call.
pub struct SkillRegistry {
    skills: HashMap<String, Box<dyn Skill>>,
}

impl SkillRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    /// Register a skill. Overwrites any existing skill with the same name.
    pub fn register(&mut self, skill: Box<dyn Skill>) {
        self.skills.insert(skill.name().to_owned(), skill);
    }

    /// Look up a skill by name.
    pub fn get(&self, name: &str) -> Option<&dyn Skill> {
        self.skills.get(name).map(|s| s.as_ref())
    }

    /// List all registered skills.
    pub fn list(&self) -> Vec<&dyn Skill> {
        self.skills.values().map(|s| s.as_ref()).collect()
    }

    /// Returns the number of registered skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Returns true if no skills are registered.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Produce OpenAI-compatible tool definitions for all registered skills.
    ///
    /// Each entry has the shape:
    /// ```json
    /// {
    ///   "type": "function",
    ///   "function": {
    ///     "name": "...",
    ///     "description": "...",
    ///     "parameters": { ... }
    ///   }
    /// }
    /// ```
    pub fn tool_definitions(&self) -> Vec<serde_json::Value> {
        self.skills
            .values()
            .map(|skill| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": skill.name(),
                        "description": skill.description(),
                        "parameters": skill.input_schema(),
                    }
                })
            })
            .collect()
    }
}

/// Build a `SkillRegistry` from the skills configuration.
///
/// Only skills with configuration present in `buddy.toml` are registered.
pub fn build_registry(config: &SkillsConfig) -> SkillRegistry {
    let mut registry = SkillRegistry::new();

    if let Some(ref cfg) = config.read_file {
        registry.register(Box::new(read_file::ReadFileSkill::new(cfg)));
    }
    if let Some(ref cfg) = config.write_file {
        registry.register(Box::new(write_file::WriteFileSkill::new(cfg)));
    }
    if let Some(ref cfg) = config.fetch_url {
        registry.register(Box::new(fetch_url::FetchUrlSkill::new(cfg)));
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial skill for testing purposes.
    struct MockSkill;

    impl Skill for MockSkill {
        fn name(&self) -> &str {
            "mock"
        }

        fn description(&self) -> &str {
            "A mock skill for testing"
        }

        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"]
            })
        }

        fn execute(
            &self,
            input: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>>
        {
            Box::pin(async move {
                let value = input
                    .get("value")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field: value".into())
                    })?;
                Ok(serde_json::json!({ "echo": value }))
            })
        }
    }

    /// A second mock skill to verify multi-skill registry behavior.
    struct AnotherSkill;

    impl Skill for AnotherSkill {
        fn name(&self) -> &str {
            "another"
        }

        fn description(&self) -> &str {
            "Another test skill"
        }

        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {},
            })
        }

        fn execute(
            &self,
            _input: serde_json::Value,
        ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>>
        {
            Box::pin(async { Ok(serde_json::json!({ "ok": true })) })
        }
    }

    #[test]
    fn registry_get_returns_some_for_registered_skill() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(MockSkill));
        assert!(registry.get("mock").is_some());
    }

    #[test]
    fn registry_get_returns_none_for_nonexistent_skill() {
        let registry = SkillRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_list_returns_all_registered_skills() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(MockSkill));
        registry.register(Box::new(AnotherSkill));
        assert_eq!(registry.list().len(), 2);
    }

    #[test]
    fn tool_definitions_has_correct_shape() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(MockSkill));

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 1);

        let def = &defs[0];
        assert_eq!(def["type"], "function");
        assert_eq!(def["function"]["name"], "mock");
        assert_eq!(def["function"]["description"], "A mock skill for testing");
        assert!(def["function"]["parameters"].is_object());
        assert_eq!(def["function"]["parameters"]["type"], "object");
    }

    #[tokio::test]
    async fn execute_mock_skill_with_valid_input() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(MockSkill));

        let skill = registry.get("mock").unwrap();
        let result = skill
            .execute(serde_json::json!({ "value": "hello" }))
            .await;

        let output = result.expect("execution should succeed");
        assert_eq!(output["echo"], "hello");
    }

    #[tokio::test]
    async fn execute_mock_skill_with_invalid_input_returns_error() {
        let mut registry = SkillRegistry::new();
        registry.register(Box::new(MockSkill));

        let skill = registry.get("mock").unwrap();
        let result = skill.execute(serde_json::json!({})).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
        assert!(err.to_string().contains("missing required field"));
    }

    #[test]
    fn skill_error_display() {
        let e1 = SkillError::InvalidInput("bad".into());
        assert_eq!(e1.to_string(), "invalid input: bad");

        let e2 = SkillError::Forbidden("nope".into());
        assert_eq!(e2.to_string(), "forbidden: nope");

        let e3 = SkillError::ExecutionFailed("boom".into());
        assert_eq!(e3.to_string(), "execution failed: boom");
    }

    #[test]
    fn build_registry_with_no_skills_is_empty() {
        let config = SkillsConfig::default();
        let registry = build_registry(&config);
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn build_registry_with_read_file_only() {
        use crate::config::ReadFileConfig;

        let config = SkillsConfig {
            read_file: Some(ReadFileConfig {
                allowed_directories: vec!["/tmp".into()],
            }),
            write_file: None,
            fetch_url: None,
        };
        let registry = build_registry(&config);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_none());
        assert!(registry.get("fetch_url").is_none());
    }

    #[test]
    fn build_registry_with_all_skills() {
        use crate::config::{FetchUrlConfig, ReadFileConfig, WriteFileConfig};

        let config = SkillsConfig {
            read_file: Some(ReadFileConfig {
                allowed_directories: vec!["/tmp".into()],
            }),
            write_file: Some(WriteFileConfig {
                allowed_directories: vec!["/tmp".into()],
            }),
            fetch_url: Some(FetchUrlConfig {
                allowed_domains: vec!["example.com".into()],
            }),
        };
        let registry = build_registry(&config);
        assert_eq!(registry.len(), 3);
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("fetch_url").is_some());
    }
}
