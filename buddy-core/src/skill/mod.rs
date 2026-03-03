pub mod fetch_url;
pub mod read_file;
pub mod recall;
pub mod remember;
pub mod working_memory;
pub mod write_file;

use std::collections::HashMap;
use std::future::Future;
use std::path::{Component, Path, PathBuf};
use std::pin::Pin;

use crate::config::SkillsConfig;
use serde::{Deserialize, Serialize};

/// Normalize a path by making it absolute and resolving `.` and `..` without
/// touching the filesystem (no symlink resolution).
pub(crate) fn normalize_path(path: &Path) -> Result<PathBuf, ToolError> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| ToolError::ExecutionFailed(format!("cannot get current directory: {e}")))?
            .join(path)
    };

    let mut components = Vec::new();
    for component in abs.components() {
        match component {
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            c => components.push(c),
        }
    }

    Ok(components.iter().collect())
}

/// Declares how a skill interacts with the outside world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    /// No side effects (e.g. read_file, memory_read).
    ReadOnly,
    /// Writes to filesystem or state (e.g. write_file, memory_write).
    Mutating,
    /// Makes outbound network requests (e.g. fetch_url).
    Network,
}

/// Errors that can occur when executing a tool.
#[derive(Debug)]
pub enum ToolError {
    /// The input provided to the tool was invalid.
    InvalidInput(String),
    /// The requested operation is not permitted.
    Forbidden(String),
    /// The tool execution failed for an operational reason.
    ExecutionFailed(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::Forbidden(msg) => write!(f, "forbidden: {msg}"),
            Self::ExecutionFailed(msg) => write!(f, "execution failed: {msg}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// A callable tool capability that can be exposed to LLM providers.
///
/// Implementors must be `Send + Sync` so they can be stored in the registry
/// and invoked from async handlers.
pub trait Tool: Send + Sync {
    /// Unique name of the tool (used as the function name in tool calls).
    fn name(&self) -> &str;

    /// Human-readable description of what the tool does.
    fn description(&self) -> &str;

    /// JSON Schema describing the expected input.
    fn input_schema(&self) -> serde_json::Value;

    /// The permission level of this tool. Defaults to `ReadOnly`.
    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::ReadOnly
    }

    /// Execute the tool with the given input and return a result.
    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, ToolError>> + Send + '_>>;
}

/// Registry of all available tools.
///
/// Tools are registered at startup and looked up by name when the LLM
/// requests a tool call.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_owned(), tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|s| s.as_ref())
    }

    /// List all registered tools.
    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|s| s.as_ref()).collect()
    }

    /// Returns the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns true if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Produce OpenAI-compatible tool definitions for all registered tools.
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
        self.tools
            .values()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.input_schema(),
                    }
                })
            })
            .collect()
    }
}

/// Build a `ToolRegistry` from the skills configuration.
///
/// Only skills with configuration present in `buddy.toml` are registered.
pub fn build_tool_registry(config: &SkillsConfig) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

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
    use crate::testutil::{MockEchoSkill, MockNoOpSkill};

    #[test]
    fn registry_get_returns_some_for_registered_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockEchoSkill));
        assert!(registry.get("echo").is_some());
    }

    #[test]
    fn registry_get_returns_none_for_nonexistent_tool() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn registry_list_returns_all_registered_tools() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockEchoSkill));
        registry.register(Box::new(MockNoOpSkill));
        assert_eq!(registry.list().len(), 2);
    }

    #[test]
    fn tool_definitions_has_correct_shape() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockEchoSkill));

        let defs = registry.tool_definitions();
        assert_eq!(defs.len(), 1);

        let def = &defs[0];
        assert_eq!(def["type"], "function");
        assert_eq!(def["function"]["name"], "echo");
        assert_eq!(def["function"]["description"], "Echoes input");
        assert!(def["function"]["parameters"].is_object());
        assert_eq!(def["function"]["parameters"]["type"], "object");
    }

    #[tokio::test]
    async fn execute_mock_tool_with_valid_input() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockEchoSkill));

        let tool = registry.get("echo").unwrap();
        let result = tool
            .execute(serde_json::json!({ "value": "hello" }))
            .await;

        let output = result.expect("execution should succeed");
        assert_eq!(output["echo"], "hello");
    }

    #[tokio::test]
    async fn execute_mock_tool_with_invalid_input_returns_error() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockEchoSkill));

        let tool = registry.get("echo").unwrap();
        let result = tool.execute(serde_json::json!({})).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
        assert!(err.to_string().contains("missing required field"));
    }

    #[test]
    fn tool_error_display() {
        let e1 = ToolError::InvalidInput("bad".into());
        assert_eq!(e1.to_string(), "invalid input: bad");

        let e2 = ToolError::Forbidden("nope".into());
        assert_eq!(e2.to_string(), "forbidden: nope");

        let e3 = ToolError::ExecutionFailed("boom".into());
        assert_eq!(e3.to_string(), "execution failed: boom");
    }

    #[test]
    fn build_tool_registry_with_no_tools_is_empty() {
        let config = SkillsConfig::default();
        let registry = build_tool_registry(&config);
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn build_tool_registry_with_read_file_only() {
        use crate::config::ReadFileConfig;

        let config = SkillsConfig {
            read_file: Some(ReadFileConfig {
                allowed_directories: vec!["/tmp".into()],
                approval: None,
            }),
            write_file: None,
            fetch_url: None,
        };
        let registry = build_tool_registry(&config);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_none());
        assert!(registry.get("fetch_url").is_none());
    }

    #[test]
    fn build_tool_registry_with_all_tools() {
        use crate::config::{FetchUrlConfig, ReadFileConfig, WriteFileConfig};

        let config = SkillsConfig {
            read_file: Some(ReadFileConfig {
                allowed_directories: vec!["/tmp".into()],
                approval: None,
            }),
            write_file: Some(WriteFileConfig {
                allowed_directories: vec!["/tmp".into()],
                approval: None,
            }),
            fetch_url: Some(FetchUrlConfig {
                allowed_domains: vec!["example.com".into()],
                approval: None,
            }),
        };
        let registry = build_tool_registry(&config);
        assert_eq!(registry.len(), 3);
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("fetch_url").is_some());
    }
}

// ============================================================================
// Skill: High-level composite operations that use tools
// ============================================================================

use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub instruction_steps: Vec<InstructionStep>,
    pub user_prompts: Vec<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstructionStep {
    Prompt { message: String },
    ToolCall { tool: String, input: serde_json::Value },
    Validate { check: String, error_message: String },
    Decision { condition: String, if_true: Vec<InstructionStep>, if_false: Vec<InstructionStep> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMatch {
    pub skill_name: String,
    pub confidence: f32,
    pub matched_keywords: Vec<String>,
}

pub struct Skill {
    definition: SkillDefinition,
    tool_registry: Arc<ToolRegistry>,
}

impl Skill {
    pub fn new(definition: SkillDefinition, tool_registry: Arc<ToolRegistry>) -> Self {
        Self { definition, tool_registry }
    }

    pub fn name(&self) -> &str {
        &self.definition.name
    }

    pub fn description(&self) -> &str {
        &self.definition.description
    }

    pub fn definition(&self) -> &SkillDefinition {
        &self.definition
    }

    pub async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, SkillError> {
        let mut current_input = input;
        let mut step_stack: Vec<&InstructionStep> = self.definition.instruction_steps.iter().collect();
        
        while let Some(step) = step_stack.pop() {
            match step {
                InstructionStep::Prompt { message } => {
                    current_input = serde_json::json!({ "prompt": message });
                }
                InstructionStep::ToolCall { tool, input: tool_input } => {
                    let tool = self.tool_registry.get(tool)
                        .ok_or_else(|| SkillError::ToolNotFound(tool.to_string()))?;
                    let merged_input = merge_input(current_input, tool_input);
                    current_input = tool.execute(merged_input).await
                        .map_err(SkillError::ToolExecutionFailed)?;
                }
                InstructionStep::Validate { check: _, error_message } => {
                    return Err(SkillError::ValidationFailed(error_message.to_string()));
                }
                InstructionStep::Decision { condition: _, if_true, if_false: _ } => {
                    for sub_step in if_true.iter().rev() {
                        step_stack.push(sub_step);
                    }
                }
            }
        }
        Ok(serde_json::json!({ "ok": true, "skill": self.definition.name }))
    }

    pub fn matches_input(&self, user_input: &str) -> SkillMatch {
        let input_lower = user_input.to_lowercase();
        let mut matched_keywords = Vec::new();
        
        for keyword in &self.definition.keywords {
            if input_lower.contains(&keyword.to_lowercase()) {
                matched_keywords.push(keyword.clone());
            }
        }
        
        let confidence = if matched_keywords.is_empty() {
            0.0
        } else {
            matched_keywords.len() as f32 / self.definition.keywords.len().max(1) as f32
        };

        SkillMatch {
            skill_name: self.definition.name.clone(),
            confidence,
            matched_keywords,
        }
    }
}

fn merge_input(context: serde_json::Value, step_input: &serde_json::Value) -> serde_json::Value {
    if context.is_object() && step_input.is_object() {
        let mut merged = context.as_object().unwrap().clone();
        if let Some(obj) = step_input.as_object() {
            for (k, v) in obj {
                merged.insert(k.clone(), v.clone());
            }
        }
        serde_json::Value::Object(merged)
    } else {
        step_input.clone()
    }
}

#[derive(Debug)]
pub enum SkillError {
    ToolNotFound(String),
    ToolExecutionFailed(crate::skill::ToolError),
    ValidationFailed(String),
    ExecutionFailed(String),
}

impl std::fmt::Display for SkillError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolNotFound(name) => write!(f, "tool not found: {name}"),
            Self::ToolExecutionFailed(e) => write!(f, "tool execution failed: {e}"),
            Self::ValidationFailed(msg) => write!(f, "validation failed: {msg}"),
            Self::ExecutionFailed(msg) => write!(f, "execution failed: {msg}"),
        }
    }
}

impl std::error::Error for SkillError {}

pub struct SkillRegistry {
    skills: HashMap<String, Skill>,
    tool_registry: Arc<ToolRegistry>,
}

impl SkillRegistry {
    pub fn new(tool_registry: Arc<ToolRegistry>) -> Self {
        Self {
            skills: HashMap::new(),
            tool_registry,
        }
    }

    pub fn register(&mut self, definition: SkillDefinition) {
        let skill = Skill::new(definition, self.tool_registry.clone());
        self.skills.insert(skill.name().to_owned(), skill);
    }

    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    pub fn find_matching(&self, user_input: &str) -> Vec<SkillMatch> {
        let mut matches: Vec<SkillMatch> = self.skills
            .values()
            .map(|skill| skill.matches_input(user_input))
            .filter(|m| m.confidence > 0.0)
            .collect();
        
        matches.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        matches
    }

    pub fn len(&self) -> usize {
        self.skills.len()
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

#[cfg(test)]
mod skill_tests {
    use super::*;
    use crate::testutil::MockEchoSkill;

    fn test_tool_registry() -> Arc<ToolRegistry> {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockEchoSkill));
        Arc::new(registry)
    }

    #[test]
    fn skill_definition_parses_from_json() {
        let json = r#"{
            "name": "create_document",
            "description": "Create a new document with content",
            "tools": ["write_file"],
            "instruction_steps": [
                {"type": "prompt", "message": "What should the document contain?"}
            ],
            "user_prompts": ["create a document"],
            "keywords": ["create", "document", "new"]
        }"#;

        let def: SkillDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(def.name, "create_document");
        assert_eq!(def.tools, vec!["write_file"]);
        assert_eq!(def.keywords, vec!["create", "document", "new"]);
    }

    #[test]
    fn skill_matches_input_with_keywords() {
        let tool_registry = test_tool_registry();
        let mut registry = SkillRegistry::new(tool_registry);
        
        registry.register(SkillDefinition {
            name: "create_document".into(),
            description: "Create a new document".into(),
            tools: vec!["write_file".into()],
            instruction_steps: vec![],
            user_prompts: vec![],
            keywords: vec!["create".into(), "document".into()],
        });

        let matches = registry.find_matching("I want to create a document");
        assert!(!matches.is_empty());
        assert_eq!(matches[0].skill_name, "create_document");
    }

    #[test]
    fn skill_does_not_match_unrelated_input() {
        let tool_registry = test_tool_registry();
        let mut registry = SkillRegistry::new(tool_registry);
        
        registry.register(SkillDefinition {
            name: "create_document".into(),
            description: "Create a new document".into(),
            tools: vec!["write_file".into()],
            instruction_steps: vec![],
            user_prompts: vec![],
            keywords: vec!["create".into(), "document".into()],
        });

        let matches = registry.find_matching("read a file");
        assert!(matches.is_empty());
    }

    #[tokio::test]
    async fn skill_executes_tool_step() {
        let tool_registry = test_tool_registry();
        let mut registry = SkillRegistry::new(tool_registry);
        
        registry.register(SkillDefinition {
            name: "echo_skill".into(),
            description: "Echo using the echo tool".into(),
            tools: vec!["echo".into()],
            instruction_steps: vec![
                InstructionStep::ToolCall {
                    tool: "echo".into(),
                    input: serde_json::json!({ "value": "hello" }),
                }
            ],
            user_prompts: vec![],
            keywords: vec!["echo".into()],
        });

        let skill = registry.get("echo_skill").unwrap();
        let result = skill.execute(serde_json::json!({})).await;
        assert!(result.is_ok());
    }
}
