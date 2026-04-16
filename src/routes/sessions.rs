use axum::{
    extract::{Path, State},
    http::HeaderMap,
    Json,
};
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        api::ApiResponse,
        lab_progress::LabProgress,
        learner_lab_status::{LearnerDashboardLab, LearnerLabStatus},
        session::{RequestHintRequest, Session, ValidateStepRequest},
    },
    services::{
        extractor::{extract_caller, Caller},
        sessions_service::SessionWithSteps,
    },
    state::AppState,
};

fn ensure_learner_role(caller: &Caller) -> Result<(), AppError> {
    if caller.roles.iter().any(|r| r == "learner") {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Learner role is required for this feature".into(),
        ))
    }
}

fn is_admin(caller: &Caller) -> bool {
    caller.roles.iter().any(|r| r == "admin")
}

fn ensure_owner(caller: &Caller, session: &Session) -> Result<(), AppError> {
    if session.user_id == caller.user_id {
        Ok(())
    } else {
        Err(AppError::Forbidden("Not session owner".to_string()))
    }
}

fn ensure_owner_or_admin(caller: &Caller, session: &Session) -> Result<(), AppError> {
    if is_admin(caller) || session.user_id == caller.user_id {
        Ok(())
    } else {
        Err(AppError::Forbidden("Forbidden".into()))
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

pub async fn get_sessions_by_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<Session>>>, AppError> {
    let caller = extract_caller(&headers)?;

    if !is_admin(&caller) {
        let creator_id = state.sessions_service.fetch_lab_creator_id(lab_id).await?;
        if creator_id != caller.user_id {
            return Err(AppError::Forbidden(
                "You are not allowed to view sessions for this lab".into(),
            ));
        }
    }

    let sessions = state.sessions_service.get_sessions_by_lab(lab_id).await?;
    Ok(Json(ApiResponse::success(sessions)))
}

pub async fn start_session(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Session>>, AppError> {
    let caller = extract_caller(&headers)?;
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
    ensure_learner_role(&caller)?;

    let status = state
        .sessions_service
        .follow_lab(caller.user_id, lab_id)
        .await?;
    Ok(Json(ApiResponse::success(status)))
}

pub async fn unfollow_lab(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_learner_role(&caller)?;

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
    ensure_learner_role(&caller)?;

    let labs = state
        .sessions_service
        .get_dashboard_labs(caller.user_id)
        .await?;

    Ok(Json(ApiResponse::success(labs)))
}

pub async fn stop_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<()>>, AppError> {
    let caller = extract_caller(&headers)?;
    let session = state.sessions_service.get_session_by_id(session_id).await?;
    ensure_owner_or_admin(&caller, &session)?;

    state.sessions_service.stop_session(session_id).await?;
    Ok(Json(ApiResponse::success(())))
}

pub async fn get_session_progress(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<LabProgress>>, AppError> {
    let caller = extract_caller(&headers)?;
    let session = state.sessions_service.get_session_by_id(session_id).await?;
    ensure_owner(&caller, &session)?;

    let progress = state.sessions_service.get_progress(session_id).await?;
    Ok(Json(ApiResponse::success(progress)))
}

pub async fn validate_step(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<ValidateStepRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let caller = extract_caller(&headers)?;
    let session = state.sessions_service.get_session_by_id(session_id).await?;
    ensure_owner(&caller, &session)?;

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
}

pub async fn request_hint(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
    Json(body): Json<RequestHintRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let caller = extract_caller(&headers)?;
    let session = state.sessions_service.get_session_by_id(session_id).await?;
    ensure_owner(&caller, &session)?;

    let (hint, cost, remaining_score) = state
        .sessions_service
        .request_hint(session_id, body.step_number, body.hint_number)
        .await?;

    Ok(Json(ApiResponse::success(serde_json::json!({
        "hint": hint,
        "cost": cost,
        "remaining_score": remaining_score
    }))))
}

pub async fn complete_session(
    State(state): State<AppState>,
    Path(session_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let caller = extract_caller(&headers)?;
    let session = state.sessions_service.get_session_by_id(session_id).await?;
    ensure_owner(&caller, &session)?;

    let stats = state.sessions_service.complete_session(session_id).await?;
    Ok(Json(ApiResponse::success(stats)))
}
