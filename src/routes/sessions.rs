use axum::{
    extract::{State, Json},
};
use uuid::Uuid;
use serde::Deserialize;

use crate::{
    state::AppState,
    models::session::Session,
    error::AppError,
};

pub async fn get_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<Session>>, AppError> {
    // MVP : user mocké (remplacé par auth plus tard)
    let user_id =
        Uuid::parse_str("456796d9-a308-4fce-8659-b70c9e17985b").unwrap();

    let sessions = state
        .sessions_service
        .list_sessions(user_id)
        .await?;

    Ok(Json(sessions))
}

#[derive(Deserialize)]
pub struct StartSessionInput {
    pub user_id: Uuid,
    pub lab_id: Uuid,
}

pub async fn start_session(
    State(state): State<AppState>,
    Json(input): Json<StartSessionInput>,
) -> Result<Json<Session>, AppError> {
    let session = state
        .sessions_service
        .start_session(input.user_id, input.lab_id)
        .await?;

    Ok(Json(session))
}

#[derive(Deserialize)]
pub struct StopSessionInput {
    pub session_id: Uuid,
}

pub async fn stop_session(
    State(state): State<AppState>,
    Json(input): Json<StopSessionInput>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .sessions_service
        .stop_session(input.session_id)
        .await?;

    Ok(Json(serde_json::json!({
        "status": "stopped",
        "session_id": input.session_id
    })))
}
