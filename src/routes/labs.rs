use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::{Serialize, Deserialize};

use crate::models::api::{ApiMeta, ApiResponse};
use crate::error::AppError;


// ======================================================
// POST /labs/:id/start (JWT, owner)
// ======================================================
pub async fn start_lab_session(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    AuthUser(claims): AuthUser,
) -> Result<Json<ApiResponse<Session>>, AppError> {
    let session = state
        .sessions_service
        .start_session(claims.user_id, lab_id)
        .await?;

    Ok(Json(ApiResponse::success(session)))
}