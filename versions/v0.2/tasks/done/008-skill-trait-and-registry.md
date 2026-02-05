# 008 — Skill Trait and Registry

## Description

Define the `Skill` trait that abstracts a callable tool capability, and build a `SkillRegistry` that collects all available skills at startup. The registry exposes skill metadata (name, description, input/output schemas) in a format that can be sent to LLM providers as tool definitions.

## Goal

A type-safe, extensible skill system exists that any new skill can plug into by implementing a single trait. The registry can enumerate all skills and look them up by name, and can serialize their schemas into the OpenAI/Anthropic tool-definition format.

## Requirements

- A `Skill` trait with:
  - `fn name(&self) -> &str`
  - `fn description(&self) -> &str`
  - `fn input_schema(&self) -> serde_json::Value` (JSON Schema)
  - `async fn execute(&self, input: serde_json::Value) -> Result<serde_json::Value, SkillError>`
- A `SkillError` enum covering: `InvalidInput(String)`, `Forbidden(String)`, `ExecutionFailed(String)`
- A `SkillRegistry` struct that:
  - Stores skills in a `HashMap<String, Box<dyn Skill>>`
  - Provides `register(skill)`, `get(name) -> Option<&dyn Skill>`, `list() -> Vec<&dyn Skill>`
  - Provides `tool_definitions() -> Vec<serde_json::Value>` that returns the skills as OpenAI-compatible tool definitions (`{ "type": "function", "function": { "name", "description", "parameters" } }`)
- The `Skill` trait must be object-safe (`Send + Sync`)
- All types defined in a `skill/` module at the crate root
- The registry is static (compiled-in skills only) — no dynamic loading

## Acceptance Criteria

- [x] `Skill` trait compiles and is object-safe (`Box<dyn Skill>` works)
- [x] `SkillRegistry` can register, retrieve, and list skills
- [x] `tool_definitions()` outputs valid OpenAI tool-definition JSON
- [x] `SkillError` variants serialize to human-readable messages
- [x] A trivial test skill can be registered and executed through the registry

## Test Cases

- Register a mock skill; call `registry.get("mock")` and assert it returns `Some`
- Call `registry.get("nonexistent")` and assert it returns `None`
- Register two skills; call `registry.list()` and assert length is 2
- Call `registry.tool_definitions()` and assert the JSON shape matches `{ "type": "function", "function": { "name": ..., "description": ..., "parameters": ... } }`
- Execute a mock skill with valid input; assert `Ok` result
- Execute a mock skill with invalid input; assert `SkillError::InvalidInput`
