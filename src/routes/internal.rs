use crate::{
    error::AppError,
    models::{
        api::ApiResponse,
        staff_analysis::{TerminalEventsIngestRequest, TerminalEventsIngestResponse},
    },
    state::AppState,
};
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

pub async fn ingest_terminal_events(
    State(state): State<AppState>,
    Json(payload): Json<TerminalEventsIngestRequest>,
) -> Result<Json<ApiResponse<TerminalEventsIngestResponse>>, AppError> {
    let result = state
        .sessions_service
        .ingest_terminal_events(payload)
        .await?;

    Ok(Json(ApiResponse::success(result)))
}
