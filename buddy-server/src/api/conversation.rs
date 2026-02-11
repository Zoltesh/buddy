//! Conversation CRUD endpoints.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use super::{ApiError, AppState, internal_error, not_found_error};
use crate::provider::Provider;

/// `GET /api/conversations` — list all conversation summaries.
pub async fn list_conversations<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<Json<Vec<buddy_core::store::ConversationSummary>>, (StatusCode, Json<ApiError>)> {
    let list = state.store.list_conversations().map_err(|e| internal_error(e))?;
    Ok(Json(list))
}

/// `POST /api/conversations` — create a new empty conversation.
pub async fn create_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Result<(StatusCode, Json<buddy_core::store::Conversation>), (StatusCode, Json<ApiError>)> {
    let conv = state.store.create_conversation("New conversation").map_err(|e| internal_error(e))?;
    Ok((StatusCode::CREATED, Json(conv)))
}

/// `GET /api/conversations/:id` — get a single conversation with all messages.
pub async fn get_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(id): Path<String>,
) -> Result<Json<buddy_core::store::Conversation>, (StatusCode, Json<ApiError>)> {
    let conv = state.store.get_conversation(&id).map_err(|e| internal_error(e))?;
    match conv {
        Some(c) => Ok(Json(c)),
        None => Err(not_found_error(format!("conversation '{id}' not found"))),
    }
}

/// `DELETE /api/conversations/:id` — delete a conversation and all messages.
pub async fn delete_conversation<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let deleted = state.store.delete_conversation(&id).map_err(|e| internal_error(e))?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found_error(format!("conversation '{id}' not found")))
    }
}
