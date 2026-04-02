use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;
use axum::http::HeaderMap;
use crate::services::extractor::extract_caller;

use crate::{
    error::AppError,
    models::{
        api::ApiResponse,
        lab_progress::LabProgress,
        learner_lab_status::{LearnerDashboardLab, LearnerLabStatus},
        session::{RequestHintRequest, Session, ValidateStepRequest},
    },
    state::AppState,
};

// ======================================================
// GET /sessions/:id (public)
// ======================================================
/*pub async fn get_session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Session>>, AppError> {
    let session = state.sessions_service.get_session_by_id(session_id).await?;

    Ok(Json(ApiResponse::success(session)))
}*/

use crate::services::sessions_service::SessionWithSteps;

// This feature is learner-scoped. Creator-only or admin-only accounts must not access it unless
// they also carry the learner role in the token.
fn ensure_learner_role(roles: &[String]) -> Result<(), AppError> {
    if roles.iter().any(|r| r == "learner") {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Learner role is required for this feature".into(),
        ))
    }
}

pub async fn get_session_by_id(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<SessionWithSteps>>, AppError> {
    let session = state
        .sessions_service
        .get_session_with_steps(session_id)
        .await?;

    Ok(Json(ApiResponse::success(session)))
}


// ======================================================
// GET /sessions/user/:id (public)
// ======================================================
/*pub async fn get_sessions_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {
    let sessions = state.sessions_service.get_sessions_by_user(user_id).await?;

    Ok(Json(ApiResponse::success(sessions)))
}*/

pub async fn get_sessions_by_user(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {

    let caller = extract_caller(&headers)?;

    if caller.user_id != user_id {
        return Err(AppError::Forbidden(
            "You can only access your own sessions".into(),
        ));
    }

    let sessions = state.sessions_service.get_sessions_by_user(user_id).await?;
    Ok(Json(ApiResponse::success(sessions)))
}

// ======================================================
// GET /sessions/lab/:id (creator)
// ======================================================
/*pub async fn get_sessions_by_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {
    let sessions = state.sessions_service.get_sessions_by_lab(lab_id).await?;

    Ok(Json(ApiResponse::success(sessions)))
}*/

pub async fn get_sessions_by_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {

    let caller = extract_caller(&headers)?;

    let is_admin = caller.roles.iter().any(|r| r == "admin");

    if !is_admin {
        let creator_id = state.sessions_service.fetch_lab_creator_id(lab_id).await?;

        if creator_id != caller.user_id {
            return Err(AppError::Forbidden(
                "You are not allowed to view sessions for this lab".into(),
            ));
        }
    }

    let sessions = state
        .sessions_service
        .get_sessions_by_lab(lab_id)
        .await?;

    Ok(Json(ApiResponse::success(sessions)))
}



// ======================================================
// POST /labs/:id/start (JWT via Gateway)
// ======================================================
/*pub async fn start_session(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    Extension(user_id): Extension<Uuid>,
) -> Result<Json<ApiResponse<Session>>, AppError> {
    let session = state
        .sessions_service
        .start_session(user_id, lab_id)
        .await?;

    Ok(Json(ApiResponse::success(session)))
}*/

pub async fn start_session(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Session>>, AppError> {
    let caller = extract_caller(&headers)?;
    // Starting a lab remains available to the existing caller path, but learner tracking is only
    // written when the token actually includes the learner role.
    let has_learner_role = caller.roles.iter().any(|r| r == "learner");

    let session = state
        .sessions_service
        .start_session(caller.user_id, lab_id, has_learner_role)
        .await?;

    Ok(Json(ApiResponse::success(session)))
}

pub async fn follow_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<LearnerLabStatus>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_learner_role(&caller.roles)?;

    // The backend derives the acting learner from the token; the client never chooses a user_id.
    let status = state.sessions_service.follow_lab(caller.user_id, lab_id).await?;
    Ok(Json(ApiResponse::success(status)))
}

pub async fn unfollow_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_learner_role(&caller.roles)?;

    state
        .sessions_service
        .unfollow_lab(caller.user_id, lab_id)
        .await?;

    Ok(Json(ApiResponse::success(())))
}

pub async fn get_learner_dashboard_labs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<LearnerDashboardLab>>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_learner_role(&caller.roles)?;

    // The dashboard is always scoped to the authenticated learner, never to an arbitrary user.
    let labs = state
        .sessions_service
        .get_dashboard_labs(caller.user_id)
        .await?;

    Ok(Json(ApiResponse::success(labs)))
}

// ======================================================
// DELETE /sessions/:id (JWT via Gateway, owner)
// ======================================================
/*pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    Extension(_user_id): Extension<Uuid>,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let session = state.sessions_service.get_session_by_id(session_id).await?;

    state.sessions_service.stop_session(session_id).await?;

    Ok(Json(ApiResponse::success(())))
}*/

pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<()>>, AppError> {

    let caller = extract_caller(&headers)?;
    let session = state.sessions_service.get_session_by_id(session_id).await?;

    let is_admin = caller.roles.iter().any(|r| r == "admin");
    let is_owner = caller.user_id == session.user_id;

    if !is_admin && !is_owner {
        return Err(AppError::Forbidden("Forbidden".into()));
    }

    state.sessions_service.stop_session(session_id).await?;
    Ok(Json(ApiResponse::success(())))
}


// ======================================================
// GET /sessions/:id/progress
// ======================================================
/*pub async fn get_session_progress(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<LabProgress>>, AppError> {
    let progress = state.sessions_service.get_progress(session_id).await?;

    Ok(Json(ApiResponse::success(progress)))
}*/


pub async fn get_session_progress(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<LabProgress>>, AppError> {

    let caller = extract_caller(&headers)?;

    let session = state
        .sessions_service
        .get_session_by_id(session_id)
        .await?;

    if session.user_id != caller.user_id {
        return Err(AppError::Forbidden(
            "Not session owner".to_string()
        ));
    }

    let progress = state
        .sessions_service
        .get_progress(session_id)
        .await?;

    Ok(Json(ApiResponse::success(progress)))
}





// ======================================================
// POST /sessions/:id/validate-step
// ======================================================
/*pub async fn validate_step(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<ValidateStepRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let result = state
        .sessions_service
        .validate_step(session_id, body.step_number, body.user_answer)
        .await?;

    if result.correct {
        Ok(Json(ApiResponse::success(serde_json::json!({
            "correct": true,
            "points_earned": result.points_earned,
            "current_step": result.current_step,
            "next_step": result.next_step
        }))))
    } else {
        Err(AppError::Conflict(format!(
            "Wrong answer (attempts: {})",
            result.attempts
        )))
    }
}*/

pub async fn validate_step(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ValidateStepRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {

    let caller = extract_caller(&headers)?;

    let session = state
        .sessions_service
        .get_session_by_id(session_id)
        .await?;

    if session.user_id != caller.user_id {
        return Err(AppError::Forbidden(
            "Not session owner".to_string()
        ));
    }

    let result = state
        .sessions_service
        .validate_step(
            session_id,
            body.step_number,
            body.user_answer
        )
        .await?;

    if result.correct {
        Ok(Json(ApiResponse::success(serde_json::json!({
            "correct": true,
            "points_earned": result.points_earned,
            "current_step": result.current_step,
            "next_step": result.next_step
        }))))
    } else {
        Err(AppError::Conflict(format!(
            "Wrong answer (attempts: {})",
            result.attempts
        )))
    }
}


// ======================================================
// POST /sessions/:id/request-hint
// ======================================================
/*pub async fn request_hint(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    Json(body): Json<RequestHintRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let (hint, cost, remaining_score) = state
        .sessions_service
        .request_hint(session_id, body.step_number, body.hint_number)
        .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "hint": hint,
        "cost": cost,
        "remaining_score": remaining_score
    }))))
}*/

pub async fn request_hint(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<RequestHintRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {

    let caller = extract_caller(&headers)?;

    let session = state
        .sessions_service
        .get_session_by_id(session_id)
        .await?;

    if session.user_id != caller.user_id {
        return Err(AppError::Forbidden(
            "Not session owner".to_string()
        ));
    }

    let (hint, cost, remaining_score) = state
        .sessions_service
        .request_hint(
            session_id,
            body.step_number,
            body.hint_number
        )
        .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "hint": hint,
        "cost": cost,
        "remaining_score": remaining_score
    }))))
}


// ======================================================
// POST /sessions/:id/complete
// ======================================================
/*pub async fn complete_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let stats = state.sessions_service.complete_session(session_id).await?;

    Ok(Json(ApiResponse::success(stats)))
}*/

pub async fn complete_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {

    let caller = extract_caller(&headers)?;

    let session = state
        .sessions_service
        .get_session_by_id(session_id)
        .await?;

    if session.user_id != caller.user_id {
        return Err(AppError::Forbidden(
            "Not session owner".to_string()
        ));
    }

    let stats = state
        .sessions_service
        .complete_session(session_id)
        .await?;

    Ok(Json(ApiResponse::success(stats)))
}
