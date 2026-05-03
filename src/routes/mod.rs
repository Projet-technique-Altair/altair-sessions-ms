/**
 * @file mod — route registration.
 *
 * @remarks
 * Registers all HTTP routes exposed by the Sessions microservice and
 * maps each endpoint to its corresponding Axum handler.
 *
 * Responsibilities:
 *
 *  - Register health and metrics endpoints
 *  - Register learner lab follow and dashboard routes
 *  - Register admin analytics and user progress inspection routes
 *  - Register session lifecycle routes
 *  - Register progress, validation, hint, and completion routes
 *  - Register session listing routes by user and lab
 *  - Register internal cron and web runtime lookup routes
 *
 * Key characteristics:
 *
 *  - Uses Axum `Router<AppState>` for shared state injection
 *  - Groups public, learner, admin, and internal endpoints
 *  - Supports both `GET`, `POST`, and `DELETE` route methods
 *  - Keeps route registration centralized in one module
 *  - Delegates all business logic to dedicated route handlers
 *
 * This module defines the HTTP surface of the Sessions microservice by
 * wiring route paths to handlers while keeping implementation details in
 * specialized route modules.
 *
 * @packageDocumentation
 */

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
