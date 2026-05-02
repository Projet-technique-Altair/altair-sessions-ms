use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

use crate::routes::{
    health::health,
    metrics::basic_metrics,
    sessions::{
        complete_session, follow_lab, get_admin_sessions_analytics, get_admin_user_dashboard_labs,
        get_learner_dashboard_labs, get_session_by_id, get_session_progress, get_sessions_by_lab,
        get_sessions_by_user, request_hint, start_session, stop_session, unfollow_lab,
        validate_step,
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
        .route("/metrics", get(basic_metrics))
        // Start lab session
        .route("/labs/:id/start", post(start_session))
        .route(
            "/learner/labs/:id/follow",
            post(follow_lab).delete(unfollow_lab),
        )
        .route("/learner/dashboard/labs", get(get_learner_dashboard_labs))
        .route(
            "/admin/users/:id/dashboard/labs",
            get(get_admin_user_dashboard_labs),
        )
        .route(
            "/admin/analytics/sessions",
            get(get_admin_sessions_analytics),
        )
        // Session lifecycle
        .route("/sessions/:id", get(get_session_by_id).delete(stop_session))
        .route("/sessions/:id/progress", get(get_session_progress))
        .route("/sessions/:id/validate-step", post(validate_step))
        .route("/sessions/:id/request-hint", post(request_hint))
        .route("/sessions/:id/complete", post(complete_session))
        // Public listings
        .route("/sessions/user/:id", get(get_sessions_by_user))
        .route("/sessions/lab/:id", get(get_sessions_by_lab))
        // For CRON
        .route(
            "/internal/cron/expire",
            post(internal::expire_sessions_cron),
        )
        // Internal runtime lookup used by lab-api-service to prepare the browser-facing
        // LAB-WEB session before redirecting the learner to the web app.
        .route(
            "/internal/sessions/:id/web-runtime",
            get(internal::get_web_runtime),
        )
}
