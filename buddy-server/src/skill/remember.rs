use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::embedding::Embedder;
use crate::memory::{VectorEntry, VectorStore};

use super::{Skill, SkillError};

/// Skill that saves facts to long-term vector memory.
pub struct RememberSkill {
    embedder: Arc<dyn Embedder>,
    vector_store: Arc<dyn VectorStore>,
}

impl RememberSkill {
    pub fn new(embedder: Arc<dyn Embedder>, vector_store: Arc<dyn VectorStore>) -> Self {
        Self {
            embedder,
            vector_store,
        }
    }
}

impl Skill for RememberSkill {
    fn name(&self) -> &str {
        "remember"
    }

    fn description(&self) -> &str {
        "Save a fact, preference, or important information to long-term memory for later retrieval across conversations."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The fact, preference, or information to remember"
                },
                "category": {
                    "type": "string",
                    "description": "Optional category label (e.g. preference, fact, project)"
                }
            },
            "required": ["text"]
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let text = input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidInput("missing required field: text".into()))?;

            if text.is_empty() {
                return Err(SkillError::InvalidInput("text must not be empty".into()));
            }

            let category = input.get("category").and_then(|v| v.as_str());
            let conversation_id = input.get("conversation_id").and_then(|v| v.as_str());

            // Embed the text.
            let embeddings = self
                .embedder
                .embed(&[text])
                .map_err(|e| SkillError::ExecutionFailed(format!("embedding failed: {e}")))?;

            let embedding = embeddings
                .into_iter()
                .next()
                .ok_or_else(|| SkillError::ExecutionFailed("embedder returned no vectors".into()))?;

            // Build metadata.
            let now = Utc::now();
            let mut metadata = serde_json::json!({
                "created_at": now.to_rfc3339(),
            });
            if let Some(cat) = category {
                metadata["category"] = serde_json::json!(cat);
            }
            if let Some(cid) = conversation_id {
                metadata["conversation_id"] = serde_json::json!(cid);
            }

            let id = Uuid::new_v4().to_string();
            let entry = VectorEntry {
                id: id.clone(),
                embedding,
                source_text: text.to_string(),
                metadata,
            };

            self.vector_store
                .store(entry)
                .map_err(|e| SkillError::ExecutionFailed(format!("failed to store memory: {e}")))?;

            Ok(serde_json::json!({
                "status": "ok",
                "id": id,
                "message": "Memory saved successfully"
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteVectorStore;
    use crate::memory::VectorStore;
    use std::sync::Mutex;

    /// Simple test embedder that returns a fixed-dimension vector.
    struct TestEmbedder {
        dims: usize,
        counter: Mutex<usize>,
    }

    impl TestEmbedder {
        fn new(dims: usize) -> Self {
            Self {
                dims,
                counter: Mutex::new(0),
            }
        }
    }

    impl Embedder for TestEmbedder {
        fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, crate::embedding::EmbedError> {
            let mut counter = self.counter.lock().unwrap();
            let mut results = Vec::new();
            for _ in texts {
                let mut vec = vec![0.0f32; self.dims];
                // Create slightly different vectors for each call.
                vec[*counter % self.dims] = 1.0;
                *counter += 1;
                results.push(vec);
            }
            Ok(results)
        }

        fn dimensions(&self) -> usize {
            self.dims
        }

        fn model_name(&self) -> &str {
            "test-embedder"
        }
    }

    fn setup() -> (Arc<dyn Embedder>, Arc<dyn VectorStore>, RememberSkill) {
        let embedder: Arc<dyn Embedder> = Arc::new(TestEmbedder::new(3));
        let store: Arc<dyn VectorStore> =
            Arc::new(SqliteVectorStore::open_in_memory("test-embedder", 3).unwrap());
        let skill = RememberSkill::new(embedder.clone(), store.clone());
        (embedder, store, skill)
    }

    #[tokio::test]
    async fn remember_stores_and_is_searchable() {
        let (_embedder, store, skill) = setup();

        skill
            .execute(serde_json::json!({
                "text": "User's favorite color is blue",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_text, "User's favorite color is blue");
    }

    #[tokio::test]
    async fn category_stored_in_metadata() {
        let (_embedder, store, skill) = setup();

        skill
            .execute(serde_json::json!({
                "text": "Likes dark mode",
                "category": "preference",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].metadata["category"], "preference");
    }

    #[tokio::test]
    async fn two_remembers_both_stored() {
        let (_embedder, store, skill) = setup();

        skill
            .execute(serde_json::json!({
                "text": "First fact",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        skill
            .execute(serde_json::json!({
                "text": "Second fact",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let meta = store.metadata().unwrap();
        assert_eq!(meta.entry_count, 2);
    }

    #[tokio::test]
    async fn empty_text_returns_invalid_input() {
        let (_embedder, _store, skill) = setup();

        let err = skill
            .execute(serde_json::json!({
                "text": "",
                "conversation_id": "conv1"
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn source_text_matches_input_exactly() {
        let (_embedder, store, skill) = setup();

        let input_text = "The project deadline is March 15th, 2025";
        skill
            .execute(serde_json::json!({
                "text": input_text,
                "conversation_id": "conv1"
            }))
            .await
            .unwrap();

        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_text, input_text);
    }
}
