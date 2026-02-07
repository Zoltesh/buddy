use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::Utc;
use rusqlite::{Connection, params};

use super::{SearchResult, StoreMetadata, VectorEntry, VectorStore, VectorStoreError};

/// SQLite-backed vector store.
///
/// Stores embeddings as blobs alongside source text and JSON metadata.
/// Similarity search uses brute-force cosine similarity computed in Rust.
pub struct SqliteVectorStore {
    conn: Mutex<Connection>,
    model_name: String,
    dimensions: usize,
    migration_required: AtomicBool,
}

impl SqliteVectorStore {
    /// Open (or create) a vector store database at `path`.
    pub fn open(path: &Path, model_name: &str, dimensions: usize) -> Result<Self, VectorStoreError> {
        let conn = Connection::open(path)
            .map_err(|e| VectorStoreError::StorageError(format!("failed to open database: {e}")))?;
        let store = Self {
            conn: Mutex::new(conn),
            model_name: model_name.to_string(),
            dimensions,
            migration_required: AtomicBool::new(false),
        };
        store.migrate()?;
        store.check_model_mismatch()?;
        Ok(store)
    }

    /// Open an in-memory vector store (for testing).
    #[cfg(test)]
    pub fn open_in_memory(model_name: &str, dimensions: usize) -> Result<Self, VectorStoreError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| VectorStoreError::StorageError(format!("failed to open in-memory db: {e}")))?;
        let store = Self {
            conn: Mutex::new(conn),
            model_name: model_name.to_string(),
            dimensions,
            migration_required: AtomicBool::new(false),
        };
        store.migrate()?;
        store.check_model_mismatch()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), VectorStoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;

            CREATE TABLE IF NOT EXISTS vectors (
                id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                source_text TEXT NOT NULL,
                metadata TEXT NOT NULL,
                model_name TEXT NOT NULL,
                dimensions INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| VectorStoreError::StorageError(format!("migration failed: {e}")))?;
        Ok(())
    }

    /// Check whether stored entries use a different model than the current one.
    fn check_model_mismatch(&self) -> Result<(), VectorStoreError> {
        let conn = self.conn.lock().unwrap();
        let result: Option<(String, i64)> = conn
            .query_row(
                "SELECT model_name, dimensions FROM vectors LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((stored_model, stored_dims)) = result {
            if stored_model != self.model_name || stored_dims as usize != self.dimensions {
                eprintln!(
                    "Warning: vector store model mismatch — stored: {} ({}d), current: {} ({}d). Run migration or clear memory.",
                    stored_model, stored_dims, self.model_name, self.dimensions
                );
                self.migration_required.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }
}

/// Encode a `Vec<f32>` as a little-endian byte blob.
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &v in embedding {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Decode a little-endian byte blob back into `Vec<f32>`.
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Compute cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

impl VectorStore for SqliteVectorStore {
    fn store(&self, entry: VectorEntry) -> Result<(), VectorStoreError> {
        if entry.embedding.len() != self.dimensions {
            return Err(VectorStoreError::DimensionMismatch {
                expected: self.dimensions,
                got: entry.embedding.len(),
            });
        }

        let conn = self.conn.lock().unwrap();
        let blob = embedding_to_bytes(&entry.embedding);
        let metadata_json = serde_json::to_string(&entry.metadata)
            .map_err(|e| VectorStoreError::StorageError(format!("failed to serialize metadata: {e}")))?;
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO vectors (id, embedding, source_text, metadata, model_name, dimensions, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.id,
                blob,
                entry.source_text,
                metadata_json,
                self.model_name,
                self.dimensions as i64,
                now,
            ],
        )
        .map_err(|e| VectorStoreError::StorageError(format!("failed to store entry: {e}")))?;

        Ok(())
    }

    fn search(
        &self,
        embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<SearchResult>, VectorStoreError> {
        if self.migration_required.load(Ordering::Relaxed) {
            return Err(VectorStoreError::MigrationRequired);
        }
        if embedding.len() != self.dimensions {
            return Err(VectorStoreError::DimensionMismatch {
                expected: self.dimensions,
                got: embedding.len(),
            });
        }

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, embedding, source_text, metadata FROM vectors")
            .map_err(|e| VectorStoreError::StorageError(format!("failed to prepare search: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                let source_text: String = row.get(2)?;
                let metadata_json: String = row.get(3)?;
                Ok((id, blob, source_text, metadata_json))
            })
            .map_err(|e| VectorStoreError::StorageError(format!("search query failed: {e}")))?;

        let mut scored: Vec<SearchResult> = Vec::new();
        for row in rows {
            let (id, blob, source_text, metadata_json) = row
                .map_err(|e| VectorStoreError::StorageError(format!("failed to read row: {e}")))?;
            let stored_embedding = bytes_to_embedding(&blob);
            let score = cosine_similarity(embedding, &stored_embedding);
            let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
                .map_err(|e| VectorStoreError::StorageError(format!("invalid metadata JSON: {e}")))?;
            scored.push(SearchResult {
                id,
                source_text,
                metadata,
                score,
            });
        }

        // Sort by descending similarity score.
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored)
    }

    fn delete(&self, id: &str) -> Result<(), VectorStoreError> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM vectors WHERE id = ?1", params![id])
            .map_err(|e| VectorStoreError::StorageError(format!("failed to delete entry: {e}")))?;
        if rows == 0 {
            return Err(VectorStoreError::NotFound(id.to_string()));
        }
        Ok(())
    }

    fn metadata(&self) -> Result<StoreMetadata, VectorStoreError> {
        let conn = self.conn.lock().unwrap();
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM vectors", [], |row| row.get(0))
            .map_err(|e| VectorStoreError::StorageError(format!("failed to count entries: {e}")))?;

        Ok(StoreMetadata {
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
            entry_count: count,
        })
    }

    fn list_all(&self) -> Result<Vec<VectorEntry>, VectorStoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, embedding, source_text, metadata FROM vectors")
            .map_err(|e| VectorStoreError::StorageError(format!("failed to prepare list_all: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                let blob: Vec<u8> = row.get(1)?;
                let source_text: String = row.get(2)?;
                let metadata_json: String = row.get(3)?;
                Ok((id, blob, source_text, metadata_json))
            })
            .map_err(|e| VectorStoreError::StorageError(format!("list_all query failed: {e}")))?;

        let mut entries = Vec::new();
        for row in rows {
            let (id, blob, source_text, metadata_json) = row
                .map_err(|e| VectorStoreError::StorageError(format!("failed to read row: {e}")))?;
            let embedding = bytes_to_embedding(&blob);
            let metadata: serde_json::Value = serde_json::from_str(&metadata_json)
                .map_err(|e| VectorStoreError::StorageError(format!("invalid metadata JSON: {e}")))?;
            entries.push(VectorEntry {
                id,
                embedding,
                source_text,
                metadata,
            });
        }
        Ok(entries)
    }

    fn clear(&self) -> Result<(), VectorStoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM vectors", [])
            .map_err(|e| VectorStoreError::StorageError(format!("failed to clear store: {e}")))?;
        self.migration_required.store(false, Ordering::Relaxed);
        Ok(())
    }

    fn needs_migration(&self) -> bool {
        self.migration_required.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> SqliteVectorStore {
        SqliteVectorStore::open_in_memory("test-model", 3).unwrap()
    }

    fn make_entry(id: &str, embedding: Vec<f32>, text: &str) -> VectorEntry {
        VectorEntry {
            id: id.to_string(),
            embedding,
            source_text: text.to_string(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn store_and_search_returns_same_entry() {
        let store = test_store();
        let embedding = vec![1.0, 0.0, 0.0];
        store
            .store(make_entry("e1", embedding.clone(), "hello"))
            .unwrap();

        let results = store.search(&embedding, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "e1");
        assert_eq!(results[0].source_text, "hello");
        assert!(
            results[0].score > 0.99,
            "searching with identical embedding should give score ~1.0, got {}",
            results[0].score
        );
    }

    #[test]
    fn search_results_ordered_by_similarity() {
        let store = test_store();

        // Query will be [1, 0, 0]. Store entries with varying similarity.
        store
            .store(make_entry("exact", vec![1.0, 0.0, 0.0], "exact match"))
            .unwrap();
        store
            .store(make_entry("close", vec![0.9, 0.1, 0.0], "close match"))
            .unwrap();
        store
            .store(make_entry("medium", vec![0.5, 0.5, 0.0], "medium match"))
            .unwrap();
        store
            .store(make_entry("far", vec![0.0, 0.0, 1.0], "far match"))
            .unwrap();
        store
            .store(make_entry("opposite", vec![-1.0, 0.0, 0.0], "opposite"))
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 5).unwrap();
        assert_eq!(results.len(), 5);
        assert_eq!(results[0].id, "exact");
        assert_eq!(results[1].id, "close");
        assert_eq!(results[2].id, "medium");
        // Verify descending order.
        for i in 0..results.len() - 1 {
            assert!(
                results[i].score >= results[i + 1].score,
                "results should be ordered by descending score"
            );
        }
    }

    #[test]
    fn delete_removes_entry_from_search() {
        let store = test_store();
        store
            .store(make_entry("d1", vec![1.0, 0.0, 0.0], "to delete"))
            .unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 1);

        store.delete("d1").unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn wrong_dimension_returns_error() {
        let store = test_store(); // 3 dimensions
        let entry = make_entry("bad", vec![1.0, 0.0], "wrong dims"); // 2 dimensions

        let err = store.store(entry).unwrap_err();
        assert!(matches!(
            err,
            VectorStoreError::DimensionMismatch {
                expected: 3,
                got: 2
            }
        ));
    }

    #[test]
    fn metadata_returns_correct_info() {
        let store = test_store();
        store
            .store(make_entry("m1", vec![1.0, 0.0, 0.0], "one"))
            .unwrap();
        store
            .store(make_entry("m2", vec![0.0, 1.0, 0.0], "two"))
            .unwrap();
        store
            .store(make_entry("m3", vec![0.0, 0.0, 1.0], "three"))
            .unwrap();

        let meta = store.metadata().unwrap();
        assert_eq!(meta.model_name, "test-model");
        assert_eq!(meta.dimensions, 3);
        assert_eq!(meta.entry_count, 3);
    }

    #[test]
    fn search_empty_store_returns_empty() {
        let store = test_store();
        let results = store.search(&[1.0, 0.0, 0.0], 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn metadata_preserved_in_search_results() {
        let store = test_store();
        let entry = VectorEntry {
            id: "meta1".to_string(),
            embedding: vec![1.0, 0.0, 0.0],
            source_text: "with metadata".to_string(),
            metadata: serde_json::json!({
                "source": "test",
                "tags": ["a", "b"],
                "count": 42
            }),
        };
        store.store(entry).unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].metadata["source"], "test");
        assert_eq!(results[0].metadata["tags"][0], "a");
        assert_eq!(results[0].metadata["tags"][1], "b");
        assert_eq!(results[0].metadata["count"], 42);
    }

    // ── Task 022: Embedding dimension tracking and migration ────────

    fn temp_db_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("buddy-test-022-{name}.db"))
    }

    #[test]
    fn mismatch_detected_when_model_changes() {
        let path = temp_db_path("mismatch");
        let _ = std::fs::remove_file(&path);

        // Store entries with model A (dim 3).
        {
            let store = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
            store.store(make_entry("e1", vec![1.0, 0.0, 0.0], "hello")).unwrap();
            store.store(make_entry("e2", vec![0.0, 1.0, 0.0], "world")).unwrap();
        }

        // Reopen with model B (dim 5). Should detect mismatch.
        let store = SqliteVectorStore::open(&path, "model-B", 5).unwrap();
        assert!(store.needs_migration(), "should detect model mismatch on startup");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn search_blocked_when_migration_required() {
        let path = temp_db_path("blocked");
        let _ = std::fs::remove_file(&path);

        {
            let store = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
            store.store(make_entry("e1", vec![1.0, 0.0, 0.0], "hello")).unwrap();
        }

        let store = SqliteVectorStore::open(&path, "model-B", 5).unwrap();
        assert!(store.needs_migration());

        let err = store.search(&[1.0, 0.0, 0.0, 0.0, 0.0], 10).unwrap_err();
        assert!(matches!(err, VectorStoreError::MigrationRequired));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn migration_re_embeds_all_entries() {
        let path = temp_db_path("migrate");
        let _ = std::fs::remove_file(&path);

        // Store 3 entries with model A (dim 3).
        {
            let store = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
            store.store(make_entry("e1", vec![1.0, 0.0, 0.0], "alpha")).unwrap();
            store.store(make_entry("e2", vec![0.0, 1.0, 0.0], "beta")).unwrap();
            store.store(make_entry("e3", vec![0.0, 0.0, 1.0], "gamma")).unwrap();
        }

        // Reopen with model B (dim 5).
        let store = SqliteVectorStore::open(&path, "model-B", 5).unwrap();
        assert!(store.needs_migration());

        // Simulate migration: read all, clear, re-store with new dims.
        let entries = store.list_all().unwrap();
        assert_eq!(entries.len(), 3);

        store.clear().unwrap();
        assert!(!store.needs_migration(), "clear should reset migration flag");

        // Store with new 5-dim embeddings.
        for (i, entry) in entries.iter().enumerate() {
            let mut new_emb = vec![0.0f32; 5];
            new_emb[i] = 1.0;
            store.store(VectorEntry {
                id: entry.id.clone(),
                embedding: new_emb,
                source_text: entry.source_text.clone(),
                metadata: entry.metadata.clone(),
            }).unwrap();
        }

        // All 3 entries should be re-stored with new dimensions.
        let meta = store.metadata().unwrap();
        assert_eq!(meta.entry_count, 3);
        assert_eq!(meta.model_name, "model-B");
        assert_eq!(meta.dimensions, 5);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn after_migration_metadata_reflects_new_model() {
        let path = temp_db_path("meta-after");
        let _ = std::fs::remove_file(&path);

        {
            let store = SqliteVectorStore::open(&path, "old-model", 3).unwrap();
            store.store(make_entry("e1", vec![1.0, 0.0, 0.0], "text")).unwrap();
        }

        let store = SqliteVectorStore::open(&path, "new-model", 4).unwrap();
        assert!(store.needs_migration());

        let entries = store.list_all().unwrap();
        store.clear().unwrap();
        for entry in &entries {
            store.store(VectorEntry {
                id: entry.id.clone(),
                embedding: vec![1.0, 0.0, 0.0, 0.0],
                source_text: entry.source_text.clone(),
                metadata: entry.metadata.clone(),
            }).unwrap();
        }

        let meta = store.metadata().unwrap();
        assert_eq!(meta.model_name, "new-model");
        assert_eq!(meta.dimensions, 4);
        assert_eq!(meta.entry_count, 1);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_empties_store_and_resets_metadata() {
        let store = test_store();
        store.store(make_entry("c1", vec![1.0, 0.0, 0.0], "one")).unwrap();
        store.store(make_entry("c2", vec![0.0, 1.0, 0.0], "two")).unwrap();

        store.clear().unwrap();

        let meta = store.metadata().unwrap();
        assert_eq!(meta.entry_count, 0);
        assert_eq!(meta.model_name, "test-model");
        assert_eq!(meta.dimensions, 3);
    }

    #[test]
    fn empty_store_adopts_current_model_on_first_write() {
        let store = SqliteVectorStore::open_in_memory("model-X", 4).unwrap();
        assert!(!store.needs_migration(), "empty store should not need migration");

        store.store(VectorEntry {
            id: "first".to_string(),
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            source_text: "first entry".to_string(),
            metadata: serde_json::json!({}),
        }).unwrap();

        let meta = store.metadata().unwrap();
        assert_eq!(meta.model_name, "model-X");
        assert_eq!(meta.dimensions, 4);
        assert_eq!(meta.entry_count, 1);
    }

    #[test]
    fn search_works_after_migration() {
        let path = temp_db_path("search-after");
        let _ = std::fs::remove_file(&path);

        {
            let store = SqliteVectorStore::open(&path, "model-A", 3).unwrap();
            store.store(make_entry("e1", vec![1.0, 0.0, 0.0], "alpha")).unwrap();
            store.store(make_entry("e2", vec![0.0, 1.0, 0.0], "beta")).unwrap();
        }

        let store = SqliteVectorStore::open(&path, "model-B", 4).unwrap();
        assert!(store.needs_migration());

        // Migrate.
        let entries = store.list_all().unwrap();
        store.clear().unwrap();
        store.store(VectorEntry {
            id: entries[0].id.clone(),
            embedding: vec![1.0, 0.0, 0.0, 0.0],
            source_text: entries[0].source_text.clone(),
            metadata: entries[0].metadata.clone(),
        }).unwrap();
        store.store(VectorEntry {
            id: entries[1].id.clone(),
            embedding: vec![0.0, 0.0, 0.0, 1.0],
            source_text: entries[1].source_text.clone(),
            metadata: entries[1].metadata.clone(),
        }).unwrap();

        // Search should now work with new dimensions.
        let results = store.search(&[1.0, 0.0, 0.0, 0.0], 10).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "e1", "closest match should be e1");
        assert!(results[0].score > 0.99);

        let _ = std::fs::remove_file(&path);
    }
}
