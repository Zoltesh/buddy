use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::provider::Provider;

use super::AppState;

/// Response body for `GET /api/embedder/health`.
#[derive(Serialize, serde::Deserialize)]
pub struct EmbedderHealthResponse {
    /// Whether an embedder is active (always true after task 042).
    pub active: bool,
    /// Provider type (e.g., "local", "openai").
    pub provider_type: String,
    /// Model name string.
    pub model_name: String,
    /// Embedding vector dimensions.
    pub dimensions: usize,
    /// Health status: "healthy" or "unhealthy".
    pub status: String,
    /// Error message when unhealthy, null when healthy.
    pub message: Option<String>,
}

/// `GET /api/embedder/health` — check if the active embedder is healthy.
///
/// This endpoint:
/// - Calls `embedder.embed(&["health check"])` to verify the embedder works
/// - Returns 200 OK with health status in the JSON payload
/// - Times out after 5 seconds if the embedder hangs
/// - Does not modify any state
pub async fn get_embedder_health<P: Provider + 'static>(
    State(state): State<Arc<AppState<P>>>,
) -> Json<EmbedderHealthResponse> {
    let embedder_opt = state.embedder.load_full();

    let embedder = match embedder_opt.as_ref() {
        Some(e) => e,
        None => {
            // No embedder configured (should not happen after task 042, but handle it)
            return Json(EmbedderHealthResponse {
                active: false,
                provider_type: "none".into(),
                model_name: "none".into(),
                dimensions: 0,
                status: "unhealthy".into(),
                message: Some("No embedder configured".into()),
            });
        }
    };

    let provider_type = embedder.provider_type().to_string();
    let model_name = embedder.model_name().to_string();
    let dimensions = embedder.dimensions();

    // Clone Arc for the async block
    let embedder_clone = Arc::clone(embedder);

    // Run the health check with a 5-second timeout
    let health_check = tokio::time::timeout(Duration::from_secs(5), async move {
        embedder_clone.embed(&["health check"])
    })
    .await;

    let (status, message) = match health_check {
        Ok(Ok(vectors)) => {
            // Check that we got exactly one vector with the expected dimensions
            if vectors.len() != 1 {
                (
                    "unhealthy".into(),
                    Some(format!(
                        "Expected 1 embedding vector, got {}",
                        vectors.len()
                    )),
                )
            } else if vectors[0].len() != dimensions {
                (
                    "unhealthy".into(),
                    Some(format!(
                        "Expected embedding dimension {}, got {}",
                        dimensions,
                        vectors[0].len()
                    )),
                )
            } else {
                ("healthy".into(), None)
            }
        }
        Ok(Err(e)) => (
            "unhealthy".into(),
            Some(format!("Embedder error: {}", e)),
        ),
        Err(_) => (
            "unhealthy".into(),
            Some("Health check timed out after 5 seconds".into()),
        ),
    };

    Json(EmbedderHealthResponse {
        active: true,
        provider_type,
        model_name,
        dimensions,
        status,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::new_pending_approvals;
    use crate::config::{Config, MemoryConfig};
    use crate::embedding::Embedder;
    use crate::skill::SkillRegistry;
    use crate::store::Store;
    use crate::testutil::{FailingEmbedder, MockEmbedder, MockProvider, WrongDimensionEmbedder};
    use axum::http::Request;
    use axum::Router;
    use axum::{body::Body, routing::get};
    use http_body_util::BodyExt;
    use std::collections::HashMap;
    use tower::ServiceExt;

    fn test_app_with_embedder(
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Router {
        let store = Store::open_in_memory().unwrap();
        let provider = MockProvider {
            tokens: vec!["ok".into()],
        };
        let registry = SkillRegistry::new();
        let config = Config::parse(
            r#"
[[models.chat.providers]]
type = "lmstudio"
model = "test-model"
endpoint = "http://localhost:1234/v1"
"#,
        )
        .unwrap();

        let state = Arc::new(AppState {
            provider: arc_swap::ArcSwap::from_pointee(provider),
            registry: arc_swap::ArcSwap::from_pointee(registry),
            store,
            embedder: arc_swap::ArcSwap::from_pointee(embedder),
            vector_store: arc_swap::ArcSwap::from_pointee(None),
            working_memory: crate::skill::working_memory::new_working_memory_map(),
            memory_config: arc_swap::ArcSwap::from_pointee(MemoryConfig::default()),
            warnings: crate::warning::new_shared_warnings(),
            pending_approvals: new_pending_approvals(),
            conversation_approvals: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            approval_overrides: arc_swap::ArcSwap::from_pointee(HashMap::new()),
            approval_timeout: Duration::from_secs(60),
            config: std::sync::RwLock::new(config),
            config_path: std::path::PathBuf::from("/tmp/test.toml"),
            on_config_change: None,
        });

        Router::new()
            .route("/api/embedder/health", get(get_embedder_health::<MockProvider>))
            .with_state(state)
    }

    #[tokio::test]
    async fn health_check_with_local_embedder() {
        let embedder = Arc::new(MockEmbedder::new(384));
        let app = test_app_with_embedder(Some(embedder));

        let req = Request::builder()
            .uri("/api/embedder/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let health: EmbedderHealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(health.active, true);
        assert_eq!(health.provider_type, "mock");
        assert_eq!(health.model_name, "test-embedder");
        assert_eq!(health.dimensions, 384);
        assert_eq!(health.status, "healthy");
        assert_eq!(health.message, None);
    }

    #[tokio::test]
    async fn health_check_with_failing_embedder() {
        let embedder = Arc::new(FailingEmbedder::new(256));
        let app = test_app_with_embedder(Some(embedder));

        let req = Request::builder()
            .uri("/api/embedder/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let health: EmbedderHealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(health.active, true);
        assert_eq!(health.provider_type, "mock-failing");
        assert_eq!(health.status, "unhealthy");
        assert!(health.message.is_some());
        assert!(health.message.unwrap().contains("mock embedder failure"));
    }

    #[tokio::test]
    async fn health_check_with_wrong_dimension_embedder() {
        // Declares 384 dims but returns 256
        let embedder = Arc::new(WrongDimensionEmbedder::new(384, 256));
        let app = test_app_with_embedder(Some(embedder));

        let req = Request::builder()
            .uri("/api/embedder/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let health: EmbedderHealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(health.active, true);
        assert_eq!(health.status, "unhealthy");
        assert!(health.message.is_some());
        let msg = health.message.unwrap();
        assert!(msg.contains("dimension"), "message: {}", msg);
    }

    #[tokio::test]
    async fn health_check_does_not_modify_state() {
        let embedder = Arc::new(MockEmbedder::new(128));
        let app = test_app_with_embedder(Some(embedder));

        // Make multiple health check requests
        for _ in 0..3 {
            let req = Request::builder()
                .uri("/api/embedder/health")
                .body(Body::empty())
                .unwrap();

            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), 200);

            let body = resp.into_body().collect().await.unwrap().to_bytes();
            let health: EmbedderHealthResponse = serde_json::from_slice(&body).unwrap();
            assert_eq!(health.status, "healthy");
        }

        // Verify that the embedder still works consistently (no state corruption)
        let req = Request::builder()
            .uri("/api/embedder/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let health: EmbedderHealthResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(health.status, "healthy");
        assert_eq!(health.dimensions, 128);
    }

    #[tokio::test]
    async fn health_check_returns_correct_dimensions() {
        let embedder = Arc::new(MockEmbedder::new(512));
        let app = test_app_with_embedder(Some(embedder));

        let req = Request::builder()
            .uri("/api/embedder/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let health: EmbedderHealthResponse = serde_json::from_slice(&body).unwrap();

        assert_eq!(health.dimensions, 512);
        assert_eq!(health.status, "healthy");
    }
}
