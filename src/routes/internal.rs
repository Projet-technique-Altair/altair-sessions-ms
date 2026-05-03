/**
 * @file internal — internal service route handlers.
 *
 * @remarks
 * Defines internal endpoints used by backend services and scheduled jobs
 * to manage runtime lifecycle operations that are not directly exposed as
 * learner-facing API features.
 *
 * Responsibilities:
 *
 *  - Trigger expiration cleanup for outdated running sessions
 *  - Return the number of sessions expired by the cleanup process
 *  - Expose active web runtime mapping for a session
 *  - Provide runtime information required by Lab API Service
 *  - Wrap internal responses in the shared `ApiResponse` envelope
 *
 * Key characteristics:
 *
 *  - Uses shared `AppState` to access the Sessions service
 *  - Supports cron-style cleanup of expired runtimes
 *  - Returns UUID-based session and user identifiers
 *  - Exposes container identifiers for LAB-WEB bootstrap flows
 *  - Keeps internal orchestration endpoints separated from public routes
 *
 * This module acts as the internal HTTP boundary for scheduled cleanup
 * and service-to-service runtime lookup operations.
 *
 * @packageDocumentation
 */

use crate::{error::AppError, models::api::ApiResponse, state::AppState};
use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

#[derive(serde::Serialize)]
pub struct ExpireResult {
    pub expired_count: usize,
}

#[derive(serde::Serialize)]
pub struct WebRuntimeResult {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub runtime_kind: String,
    pub container_id: String,
    pub status: String,
}

pub async fn expire_sessions_cron(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ExpireResult>>, AppError> {
    let expired = state.sessions_service.expire_all_expired_sessions().await?;

    Ok(Json(ApiResponse::success(ExpireResult {
        expired_count: expired,
    })))
}

pub async fn get_web_runtime(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<WebRuntimeResult>>, AppError> {
    // web-runtime gives lab-api-service the active runtime mapping it needs to
    // bootstrap the browser session before redirecting the learner to LAB-WEB.
    let runtime = state.sessions_service.get_web_runtime(session_id).await?;

    Ok(Json(ApiResponse::success(WebRuntimeResult {
        session_id: runtime.session_id,
        user_id: runtime.user_id,
        runtime_kind: runtime.runtime_kind,
        container_id: runtime.container_id,
        status: runtime.status,
    })))
}
