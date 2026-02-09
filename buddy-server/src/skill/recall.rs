use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::embedding::Embedder;
use crate::memory::VectorStore;

use super::{Skill, SkillError};

/// Default maximum number of results returned by a recall query.
const DEFAULT_LIMIT: usize = 5;

/// Skill that searches long-term vector memory for relevant information.
pub struct RecallSkill {
    embedder: Arc<dyn Embedder>,
    vector_store: Arc<dyn VectorStore>,
}

impl RecallSkill {
    pub fn new(embedder: Arc<dyn Embedder>, vector_store: Arc<dyn VectorStore>) -> Self {
        Self {
            embedder,
            vector_store,
        }
    }
}

impl Skill for RecallSkill {
    fn name(&self) -> &str {
        "recall"
    }

    fn description(&self) -> &str {
        "Search long-term memory for previously stored facts, preferences, or context relevant to a query."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search text to find relevant memories"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 5)"
                }
            },
            "required": ["query"]
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let query = input
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidInput("missing required field: query".into()))?;

            if query.is_empty() {
                return Err(SkillError::InvalidInput("query must not be empty".into()));
            }

            let limit = input
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(DEFAULT_LIMIT);

            // Embed the query.
            let embeddings = self
                .embedder
                .embed(&[query])
                .map_err(|e| SkillError::ExecutionFailed(format!("embedding failed: {e}")))?;

            let embedding = embeddings
                .into_iter()
                .next()
                .ok_or_else(|| SkillError::ExecutionFailed("embedder returned no vectors".into()))?;

            // Search the vector store.
            let results = self
                .vector_store
                .search(&embedding, limit)
                .map_err(|e| SkillError::ExecutionFailed(format!("search failed: {e}")))?;

            let formatted: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    let mut entry = serde_json::json!({
                        "text": r.source_text,
                        "score": r.score,
                    });
                    if let Some(cat) = r.metadata.get("category") {
                        entry["category"] = cat.clone();
                    }
                    if let Some(created) = r.metadata.get("created_at") {
                        entry["created_at"] = created.clone();
                    }
                    entry
                })
                .collect();

            let total = formatted.len();
            Ok(serde_json::json!({
                "results": formatted,
                "total_found": total,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::sqlite::SqliteVectorStore;
    use crate::memory::{VectorEntry, VectorStore};
    use crate::skill::Skill;
    use crate::testutil::MockEmbedder;

    fn setup() -> (Arc<dyn Embedder>, Arc<dyn VectorStore>, RecallSkill) {
        let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(3));
        let store: Arc<dyn VectorStore> =
            Arc::new(SqliteVectorStore::open_in_memory("test-embedder", 3).unwrap());
        let skill = RecallSkill::new(embedder.clone(), store.clone());
        (embedder, store, skill)
    }

    fn store_entry(store: &dyn VectorStore, id: &str, embedding: Vec<f32>, text: &str, category: Option<&str>) {
        let mut metadata = serde_json::json!({ "created_at": "2025-01-15T10:00:00Z" });
        if let Some(cat) = category {
            metadata["category"] = serde_json::json!(cat);
        }
        store
            .store(VectorEntry {
                id: id.to_string(),
                embedding,
                source_text: text.to_string(),
                metadata,
            })
            .unwrap();
    }

    #[tokio::test]
    async fn recall_finds_stored_memories() {
        let (_embedder, store, skill) = setup();

        store_entry(&*store, "m1", vec![1.0, 0.0, 0.0], "favorite color is blue", Some("preference"));
        store_entry(&*store, "m2", vec![0.0, 1.0, 0.0], "likes pizza", Some("preference"));
        store_entry(&*store, "m3", vec![0.0, 0.0, 1.0], "works at Acme", Some("fact"));

        // Query vector [1, 0, 0] should match m1 best.
        // The embedder's next call (counter=3) gives vec[0]=1.0 which matches m1.
        let result = skill
            .execute(serde_json::json!({ "query": "what color" }))
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0]["score"].as_f64().unwrap() > 0.0);
        assert!(results[0]["text"].as_str().is_some());
    }

    #[tokio::test]
    async fn limit_caps_results() {
        let (_embedder, store, skill) = setup();

        store_entry(&*store, "m1", vec![1.0, 0.0, 0.0], "one", None);
        store_entry(&*store, "m2", vec![0.0, 1.0, 0.0], "two", None);
        store_entry(&*store, "m3", vec![0.0, 0.0, 1.0], "three", None);

        let result = skill
            .execute(serde_json::json!({ "query": "test", "limit": 1 }))
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(result["total_found"], 1);
    }

    #[tokio::test]
    async fn default_limit_is_five() {
        let (_embedder, store, skill) = setup();

        // Store 7 entries.
        for i in 0..7 {
            let mut emb = vec![0.0f32; 3];
            emb[i % 3] = 1.0;
            store_entry(&*store, &format!("m{i}"), emb, &format!("entry {i}"), None);
        }

        let result = skill
            .execute(serde_json::json!({ "query": "test" }))
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert!(results.len() <= 5, "default limit should be 5, got {}", results.len());
    }

    #[tokio::test]
    async fn empty_store_returns_empty_results() {
        let (_embedder, _store, skill) = setup();

        let result = skill
            .execute(serde_json::json!({ "query": "anything" }))
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert!(results.is_empty());
        assert_eq!(result["total_found"], 0);
    }

    #[tokio::test]
    async fn empty_query_returns_invalid_input() {
        let (_embedder, _store, skill) = setup();

        let err = skill
            .execute(serde_json::json!({ "query": "" }))
            .await
            .unwrap_err();
        assert!(matches!(err, SkillError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn results_include_category_and_timestamp() {
        let (_embedder, store, skill) = setup();

        store_entry(&*store, "m1", vec![1.0, 0.0, 0.0], "blue is favorite", Some("preference"));

        let result = skill
            .execute(serde_json::json!({ "query": "color" }))
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["category"], "preference");
        assert_eq!(results[0]["created_at"], "2025-01-15T10:00:00Z");
    }

    #[tokio::test]
    async fn results_ordered_by_similarity() {
        let (_embedder, store, skill) = setup();

        // Store entries. The query will get embedding [0,0,1] (counter position 3 % 3 = 0 → wait...
        // Actually with counter starting at 0, the 3 stores use positions 0,1,2.
        // The recall query uses position 3 → 3%3=0 → [1,0,0].
        // So m1 (exact [1,0,0]) should rank first.
        store_entry(&*store, "m1", vec![1.0, 0.0, 0.0], "exact match", None);
        store_entry(&*store, "m2", vec![0.5, 0.5, 0.0], "partial match", None);
        store_entry(&*store, "m3", vec![0.0, 0.0, 1.0], "no match", None);

        let result = skill
            .execute(serde_json::json!({ "query": "search" }))
            .await
            .unwrap();

        let results = result["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        // Verify descending score order.
        for i in 0..results.len() - 1 {
            let s1 = results[i]["score"].as_f64().unwrap();
            let s2 = results[i + 1]["score"].as_f64().unwrap();
            assert!(s1 >= s2, "results should be ordered by descending score");
        }
    }
}
