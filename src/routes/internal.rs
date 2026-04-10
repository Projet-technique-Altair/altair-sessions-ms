use crate::{
    error::AppError,
    models::{api::ApiResponse, session::RuntimeLookup},
    state::AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};

#[derive(serde::Serialize)]
pub struct ExpireResult {
    pub expired_count: usize,
}

pub async fn expire_sessions_cron(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ExpireResult>>, AppError> {
    let expired = state.sessions_service.expire_all_expired_sessions().await?;

    Ok(Json(ApiResponse::success(ExpireResult {
        expired_count: expired,
    })))
}

pub async fn get_runtime_by_container_id(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
) -> Result<Json<ApiResponse<RuntimeLookup>>, AppError> {
    // This internal lookup gives lab-api-service the minimal ownership
    // snapshot it needs before issuing a browser cookie for a web runtime.
    let runtime = state
        .sessions_service
        .get_active_runtime_by_container_id(&container_id)
        .await?;

    Ok(Json(ApiResponse::success(runtime)))
}
