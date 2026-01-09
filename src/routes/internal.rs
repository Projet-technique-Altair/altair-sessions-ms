use axum::{
    extract::State,
    Json,
};
use crate::{
    state::AppState,
    error::AppError,
    models::api::ApiResponse,
};

#[derive(serde::Serialize)]
pub struct ExpireResult {
    pub expired_count: usize,
}

pub async fn expire_sessions_cron(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ExpireResult>>, AppError> {

    let expired = state
        .sessions_service
        .expire_all_expired_sessions()
        .await?;

    Ok(Json(ApiResponse::success(ExpireResult {
        expired_count: expired,
    })))
}
