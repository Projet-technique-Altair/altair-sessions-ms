use axum::{
    Router,
    routing::{post, get},
    Json,
    extract::{Path, State},
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    models::session::LabSession,
    state::AppState,
};

pub fn sessions_routes() -> Router<AppState> {
    Router::new()
        .route("/labs/:lab_id/start", post(start_session))
        .route("/sessions", get(list_sessions))
}

async fn start_session(
    Path(lab_id): Path<String>,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {

    let mut sessions = state.sessions.lock().unwrap();

    let session = LabSession {
        session_id: Uuid::new_v4().to_string(),
        user_id: "mock-user".into(),
        lab_id,
        container_id: "mock-container".into(),
        status: "running".into(),
        webshell_url: "ws://localhost:3000/ws/mock".into(),
    };

    sessions.push(session.clone());

    Json(json!(session))
}

async fn list_sessions(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let sessions = state.sessions.lock().unwrap();
    Json(json!(*sessions))
}
