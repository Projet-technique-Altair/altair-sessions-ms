use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

use crate::routes::{
    health::health,
    sessions::{
        get_session_by_id, get_sessions_by_lab, get_sessions_by_user, start_session, stop_session,
    },
};

pub mod health;
pub mod internal;
pub mod metrics;
pub mod sessions;

pub fn init_routes() -> Router<AppState> {
    Router::new()
        // Health
        .route("/health", get(health))
        // Start lab session
        .route("/labs/:id/start", post(start_session))
        // Session lifecycle
        .route("/sessions/:id", get(get_session_by_id).delete(stop_session))
        // Public listings
        .route("/sessions/user/:id", get(get_sessions_by_user))
        .route("/sessions/lab/:id", get(get_sessions_by_lab))
        // For CRON
        .route(
            "/internal/cron/expire",
            post(internal::expire_sessions_cron),
        )
}
