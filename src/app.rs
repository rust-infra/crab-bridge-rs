//! Axum router wiring shared by the binary and integration tests.

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::get,
};

use crate::admin::{dashboard_page, overview, prometheus_metrics};
use crate::handlers::{api_root, handle_fallback, handle_models, handle_responses, health};
use crate::state::AppState;

/// Build the CrabBridge HTTP router (routes + body limit; no CORS or rate limiting).
pub fn build_router(state: AppState, admin_enabled: bool) -> Router {
    let mut router = Router::new()
        .route("/health", get(health))
        .route("/v1", get(api_root))
        .route("/v1/responses", axum::routing::post(handle_responses))
        .route("/v1/models", get(handle_models))
        .route("/{provider}/v1", get(api_root))
        .route("/{provider}/v1/responses", axum::routing::post(handle_responses))
        .route("/{provider}/v1/models", get(handle_models));

    if admin_enabled {
        router = router
            .route("/admin", get(dashboard_page))
            .route("/admin/api/overview", get(overview))
            .route("/metrics", get(prometheus_metrics));
    }

    router
        .fallback(handle_fallback)
        .layer(DefaultBodyLimit::disable())
        .with_state(state)
}
