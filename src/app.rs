//! Axum router wiring shared by the binary and integration tests.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::handlers::{api_root, handle_fallback, handle_models, handle_responses, health};
use crate::state::AppState;

/// Build the CrabBridge HTTP router (routes + body limit; no CORS or rate limiting).
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1", get(api_root))
        .route("/v1/responses", post(handle_responses))
        .route("/v1/models", get(handle_models))
        .route("/{provider}/v1", get(api_root))
        .route("/{provider}/v1/responses", post(handle_responses))
        .route("/{provider}/v1/models", get(handle_models))
        .fallback(handle_fallback)
        .layer(DefaultBodyLimit::disable())
        .with_state(state)
}
