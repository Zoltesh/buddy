//! Memory management endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use super::{ApiError, AppState, internal_error};
use buddy_core::provider::Provider;

/// `POST /api/memory/migrate` — re-embed all stored memories using the current model.
pub async fn migrate_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let vs_snap = state.vector_store.load();
    let emb_snap = state.embedder.load();
    let vector_store = vs_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;
    let embedder = emb_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_embedder".into(),
                message: "no embedder configured".into(),
            }),
        )
    })?;

    // Read all existing entries.
    let entries = vector_store.list_all().map_err(|e| {
        internal_error(format!("failed to list entries: {e}"))
    })?;

    // Collect source texts for re-embedding.
    let texts: Vec<&str> = entries.iter().map(|e| e.source_text.as_str()).collect();

    let new_embeddings = if texts.is_empty() {
        Vec::new()
    } else {
        embedder.embed(&texts).map_err(|e| {
            internal_error(format!("re-embedding failed: {e}"))
        })?
    };

    // Clear and re-store with new embeddings.
    vector_store.clear().map_err(|e| {
        internal_error(format!("failed to clear store: {e}"))
    })?;

    let count = entries.len();
    for (entry, embedding) in entries.into_iter().zip(new_embeddings) {
        let new_entry = buddy_core::memory::VectorEntry {
            id: entry.id,
            embedding,
            source_text: entry.source_text,
            metadata: entry.metadata,
        };
        vector_store.store(new_entry).map_err(|e| {
            internal_error(format!("failed to store migrated entry: {e}"))
        })?;
    }

    Ok(Json(serde_json::json!({ "migrated": count })))
}

/// `DELETE /api/memory` — clear all stored memories.
pub async fn clear_memory<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let vs_snap = state.vector_store.load();
    let vector_store = vs_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;

    vector_store.clear().map_err(|e| {
        internal_error(format!("failed to clear memory: {e}"))
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Response for `GET /api/memory/status`.
#[derive(Serialize)]
pub struct MemoryStatusResponse {
    pub total_entries: usize,
    pub migration_required: bool,
    pub stored_model: Option<String>,
    pub stored_dimensions: Option<usize>,
    pub active_model: Option<String>,
    pub active_dimensions: Option<usize>,
}

/// `GET /api/memory/status` — get current memory and migration status.
pub async fn get_memory_status<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<MemoryStatusResponse>, (StatusCode, Json<ApiError>)> {
    let vs_snap = state.vector_store.load();
    let vector_store = vs_snap.as_ref().as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "no_vector_store".into(),
                message: "no vector store configured".into(),
            }),
        )
    })?;

    let emb_snap = state.embedder.load();
    let embedder = emb_snap.as_ref();

    let total_entries = vector_store.count().map_err(|e| {
        internal_error(format!("failed to count entries: {e}"))
    })?;

    let migration_required = vector_store.needs_migration() && total_entries > 0;

    // Get stored model/dimensions from the actual stored vectors.
    let (stored_model, stored_dimensions) = match vector_store.stored_model_info() {
        Ok(Some(info)) => (Some(info.model_name), Some(info.dimensions)),
        Ok(None) => (None, None),
        Err(_) => (None, None),
    };

    // Get active embedder model/dimensions.
    let (active_model, active_dimensions) = if let Some(emb) = embedder.as_ref() {
        (Some(emb.model_name().to_string()), Some(emb.dimensions()))
    } else {
        (None, None)
    };

    Ok(Json(MemoryStatusResponse {
        total_entries,
        migration_required,
        stored_model,
        stored_dimensions,
        active_model,
        active_dimensions,
    }))
}
