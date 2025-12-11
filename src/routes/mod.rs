use axum::{
    Router,
    routing::{get, post},
};

use crate::state::AppState;

pub mod health;
pub mod metrics;
pub mod sessions;

pub fn init_routes() -> Router<AppState> {
    Router::new()
        // Health
        .route("/health", get(health::health))

        // Sessions routes
        .route("/sessions", get(sessions::get_sessions))
        .route("/sessions/start", post(sessions::start_session))
        .route("/sessions/stop", post(sessions::stop_session))
}