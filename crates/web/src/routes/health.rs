use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

async fn health(State(state): State<AppState>) -> (StatusCode, &'static str) {
    if state.infrastructure.is_healthy().await {
        (StatusCode::OK, "Healthy")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "Unhealthy")
    }
}
