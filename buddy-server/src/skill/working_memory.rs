use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::{PermissionLevel, Skill, SkillError};

/// Per-conversation short-term scratchpad.
///
/// Holds key-value pairs and free-form notes that persist within a
/// conversation but are cleared on server restart.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct WorkingMemory {
    entries: HashMap<String, String>,
    notes: Vec<String>,
}

impl WorkingMemory {
    pub fn set(&mut self, key: String, value: String) {
        self.entries.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }

    pub fn list_keys(&self) -> Vec<&str> {
        self.entries.keys().map(|k| k.as_str()).collect()
    }

    pub fn add_note(&mut self, text: String) {
        self.notes.push(text);
    }

    pub fn get_notes(&self) -> &[String] {
        &self.notes
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.notes.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.notes.is_empty()
    }

    /// Serialize the scratchpad contents for inclusion in a system prompt.
    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();
        if !self.entries.is_empty() {
            parts.push("Key-value pairs:".to_string());
            let mut keys: Vec<&str> = self.entries.keys().map(|k| k.as_str()).collect();
            keys.sort();
            for key in keys {
                parts.push(format!("  {}: {}", key, self.entries[key]));
            }
        }
        if !self.notes.is_empty() {
            parts.push("Notes:".to_string());
            for note in &self.notes {
                parts.push(format!("  - {note}"));
            }
        }
        parts.join("\n")
    }
}

/// Shared map of per-conversation working memory.
pub type WorkingMemoryMap = Arc<Mutex<HashMap<String, WorkingMemory>>>;

/// Create an empty working memory map.
pub fn new_working_memory_map() -> WorkingMemoryMap {
    Arc::new(Mutex::new(HashMap::new()))
}

// ── memory_write skill ─────────────────────────────────────────────────

pub struct MemoryWriteSkill {
    map: WorkingMemoryMap,
}

impl MemoryWriteSkill {
    pub fn new(map: WorkingMemoryMap) -> Self {
        Self { map }
    }
}

impl Skill for MemoryWriteSkill {
    fn name(&self) -> &str {
        "memory_write"
    }

    fn description(&self) -> &str {
        "Write to the conversation's working memory scratchpad. Supports set (key-value), note (free-form), delete (remove key), and clear (wipe all)."
    }

    fn permission_level(&self) -> PermissionLevel {
        PermissionLevel::Mutating
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["set", "note", "delete", "clear"],
                    "description": "The action to perform"
                },
                "key": {
                    "type": "string",
                    "description": "Key name (required for set and delete)"
                },
                "value": {
                    "type": "string",
                    "description": "Value to store (required for set and note)"
                }
            },
            "required": ["action"]
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let action = input
                .get("action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidInput("missing required field: action".into()))?;

            let conversation_id = input
                .get("conversation_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SkillError::ExecutionFailed("missing conversation context".into())
                })?;

            let mut map = self.map.lock().unwrap();
            let wm = map.entry(conversation_id.to_string()).or_default();

            match action {
                "set" => {
                    let key = input
                        .get("key")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            SkillError::InvalidInput("set requires 'key'".into())
                        })?;
                    let value = input
                        .get("value")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            SkillError::InvalidInput("set requires 'value'".into())
                        })?;
                    wm.set(key.to_string(), value.to_string());
                    Ok(serde_json::json!({ "status": "ok", "action": "set", "key": key, "value": value }))
                }
                "note" => {
                    let value = input
                        .get("value")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            SkillError::InvalidInput("note requires 'value'".into())
                        })?;
                    wm.add_note(value.to_string());
                    Ok(serde_json::json!({ "status": "ok", "action": "note" }))
                }
                "delete" => {
                    let key = input
                        .get("key")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            SkillError::InvalidInput("delete requires 'key'".into())
                        })?;
                    let existed = wm.delete(key);
                    Ok(serde_json::json!({ "status": "ok", "action": "delete", "key": key, "existed": existed }))
                }
                "clear" => {
                    wm.clear();
                    Ok(serde_json::json!({ "status": "ok", "action": "clear" }))
                }
                other => Err(SkillError::InvalidInput(format!(
                    "unknown action: '{other}'. Valid actions: set, note, delete, clear"
                ))),
            }
        })
    }
}

// ── memory_read skill ──────────────────────────────────────────────────

pub struct MemoryReadSkill {
    map: WorkingMemoryMap,
}

impl MemoryReadSkill {
    pub fn new(map: WorkingMemoryMap) -> Self {
        Self { map }
    }
}

impl Skill for MemoryReadSkill {
    fn name(&self) -> &str {
        "memory_read"
    }

    fn description(&self) -> &str {
        "Read from the conversation's working memory scratchpad. Provide a key to read a specific value, or omit it to get the full scratchpad contents."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Key to look up (omit to return all stored data)"
                }
            }
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let conversation_id = input
                .get("conversation_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SkillError::ExecutionFailed("missing conversation context".into())
                })?;

            let map = self.map.lock().unwrap();
            let wm = map.get(conversation_id);

            if let Some(key) = input.get("key").and_then(|v| v.as_str()) {
                let value = wm.and_then(|w| w.get(key));
                match value {
                    Some(v) => Ok(serde_json::json!({ "key": key, "value": v })),
                    None => Ok(serde_json::json!({ "key": key, "value": null, "message": "not found" })),
                }
            } else {
                match wm {
                    Some(w) if !w.is_empty() => {
                        Ok(serde_json::json!({
                            "entries": w.entries,
                            "notes": w.get_notes(),
                        }))
                    }
                    _ => Ok(serde_json::json!({ "entries": {}, "notes": [] })),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (WorkingMemoryMap, MemoryWriteSkill, MemoryReadSkill) {
        let map = new_working_memory_map();
        let write = MemoryWriteSkill::new(map.clone());
        let read = MemoryReadSkill::new(map.clone());
        (map, write, read)
    }

    #[tokio::test]
    async fn set_and_read_key() {
        let (_map, write, read) = setup();

        let result = write
            .execute(serde_json::json!({
                "action": "set",
                "key": "name",
                "value": "Alice",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");

        let result = read
            .execute(serde_json::json!({ "key": "name", "conversation_id": "conv1" }))
            .await
            .unwrap();
        assert_eq!(result["value"], "Alice");
    }

    #[tokio::test]
    async fn note_appears_in_full_read() {
        let (_map, write, read) = setup();

        write
            .execute(serde_json::json!({
                "action": "note",
                "value": "User prefers dark mode",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let result = read
            .execute(serde_json::json!({ "conversation_id": "conv1" }))
            .await
            .unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0], "User prefers dark mode");
    }

    #[tokio::test]
    async fn delete_removes_key() {
        let (_map, write, read) = setup();

        write
            .execute(serde_json::json!({
                "action": "set",
                "key": "name",
                "value": "Alice",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        write
            .execute(serde_json::json!({
                "action": "delete",
                "key": "name",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let result = read
            .execute(serde_json::json!({ "key": "name", "conversation_id": "conv1" }))
            .await
            .unwrap();
        assert!(result["value"].is_null());
        assert_eq!(result["message"], "not found");
    }

    #[tokio::test]
    async fn clear_empties_scratchpad() {
        let (_map, write, read) = setup();

        write
            .execute(serde_json::json!({
                "action": "set",
                "key": "a",
                "value": "1",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();
        write
            .execute(serde_json::json!({
                "action": "note",
                "value": "a note",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        write
            .execute(serde_json::json!({
                "action": "clear",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let result = read
            .execute(serde_json::json!({ "conversation_id": "conv1" }))
            .await
            .unwrap();
        let entries = result["entries"].as_object().unwrap();
        let notes = result["notes"].as_array().unwrap();
        assert!(entries.is_empty());
        assert!(notes.is_empty());
    }

    #[tokio::test]
    async fn per_conversation_isolation() {
        let (_map, write, read) = setup();

        write
            .execute(serde_json::json!({
                "action": "set",
                "key": "name",
                "value": "Alice",
                "conversation_id": "conv_A"
            }))
            .await
            .unwrap();

        // Read from a different conversation — should be empty.
        let result = read
            .execute(serde_json::json!({ "key": "name", "conversation_id": "conv_B" }))
            .await
            .unwrap();
        assert!(result["value"].is_null());
    }

    #[tokio::test]
    async fn invalid_action_returns_error() {
        let (_map, write, _read) = setup();

        let err = write
            .execute(serde_json::json!({
                "action": "foo",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn set_without_key_returns_error() {
        let (_map, write, _read) = setup();

        let err = write
            .execute(serde_json::json!({
                "action": "set",
                "value": "no key",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }
}
