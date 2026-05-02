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
        staff_analysis::{
            AiAnalysisFinalResponse, GroupActivityMetrics, GroupActivityResponse,
            LabStaffAnalytics, StudentActivityMetrics, StudentActivityResponse,
        },
    },
    services::extractor::{extract_caller, Caller},
    state::AppState,
};

fn ensure_staff_role(caller: &Caller) -> Result<(), AppError> {
    if caller
        .roles
        .iter()
        .any(|role| role == "creator" || role == "admin")
    {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "Creator or admin role is required for staff analytics".into(),
        ))
    }
}

fn is_admin(caller: &Caller) -> bool {
    caller.roles.iter().any(|role| role == "admin")
}

fn roles_header(caller: &Caller) -> String {
    caller.roles.join(",")
}

pub async fn get_lab_analytics(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<LabStaffAnalytics>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_staff_role(&caller)?;

    let analytics = state
        .sessions_service
        .get_staff_lab_analytics(caller.user_id, is_admin(&caller), lab_id)
        .await?;

    Ok(Json(ApiResponse::success(analytics)))
}

pub async fn get_student_activity(
    State(state): State<AppState>,
    Path((lab_id, student_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<StudentActivityResponse>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_staff_role(&caller)?;
    let roles = roles_header(&caller);

    let activity = state
        .sessions_service
        .get_staff_student_activity(
            caller.user_id,
            &roles,
            is_admin(&caller),
            lab_id,
            student_id,
        )
        .await?;

    Ok(Json(ApiResponse::success(activity)))
}

pub async fn get_group_activity(
    State(state): State<AppState>,
    Path((lab_id, group_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<GroupActivityResponse>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_staff_role(&caller)?;
    let roles = roles_header(&caller);

    let activity = state
        .sessions_service
        .get_staff_group_activity(caller.user_id, &roles, is_admin(&caller), lab_id, group_id)
        .await?;

    Ok(Json(ApiResponse::success(activity)))
}

pub async fn get_common_blockers(
    State(state): State<AppState>,
    Path(lab_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<String>>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_staff_role(&caller)?;

    let blockers = state
        .sessions_service
        .get_staff_common_blockers(caller.user_id, is_admin(&caller), lab_id)
        .await?;

    Ok(Json(ApiResponse::success(blockers)))
}

pub async fn generate_student_ai_analysis(
    State(state): State<AppState>,
    Path((lab_id, student_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<AiAnalysisFinalResponse<StudentActivityMetrics>>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_staff_role(&caller)?;
    let roles = roles_header(&caller);

    let report = state
        .sessions_service
        .generate_staff_student_ai_analysis(
            caller.user_id,
            &roles,
            is_admin(&caller),
            lab_id,
            student_id,
        )
        .await?;

    Ok(Json(ApiResponse::success(report)))
}

pub async fn generate_group_ai_analysis(
    State(state): State<AppState>,
    Path((lab_id, group_id)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<AiAnalysisFinalResponse<GroupActivityMetrics>>>, AppError> {
    let caller = extract_caller(&headers)?;
    ensure_staff_role(&caller)?;
    let roles = roles_header(&caller);

    let report = state
        .sessions_service
        .generate_staff_group_ai_analysis(
            caller.user_id,
            &roles,
            is_admin(&caller),
            lab_id,
            group_id,
        )
        .await?;

    Ok(Json(ApiResponse::success(report)))
}
