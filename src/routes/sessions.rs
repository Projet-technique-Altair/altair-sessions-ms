use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::{
    state::AppState,
    error::AppError,
    models::{
        api::ApiResponse,
        auth::AuthUser,
        session::Session,
    },
};

// ======================================================
// GET /sessions/:id (public)
// ======================================================
pub async fn get_session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Session>>, AppError> {
    let session = state
        .sessions_service
        .get_session_by_id(session_id)
        .await?;

    Ok(Json(ApiResponse::success(session)))
}

// ======================================================
// GET /sessions/user/:id (public)
// ======================================================
pub async fn get_sessions_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {
    let sessions = state
        .sessions_service
        .get_sessions_by_user(user_id)
        .await?;

    Ok(Json(ApiResponse::success(sessions)))
}

// ======================================================
// GET /sessions/lab/:id (creator)
// ======================================================
pub async fn get_sessions_by_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {
    let sessions = state
        .sessions_service
        .get_sessions_by_lab(lab_id)
        .await?;

    Ok(Json(ApiResponse::success(sessions)))
}


// ======================================================
// POST /labs/:id/start (JWT)
// ======================================================
pub async fn start_session(
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



// ======================================================
// DELETE /sessions/:id (JWT, owner)
// ======================================================
pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    AuthUser(claims): AuthUser,
) -> Result<Json<ApiResponse<()>>, AppError> {

    let session = state
        .sessions_service
        .get_session_by_id(session_id)
        .await?;

    // 🔐 Ownership strict
    if session.user_id != claims.user_id {
        return Err(AppError::Forbidden("Not session owner".into()));
    }

    state
        .sessions_service
        .stop_session(session_id)
        .await?;

    Ok(Json(ApiResponse::success(())))
}
