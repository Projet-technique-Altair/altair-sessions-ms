use axum::{Router, routing::{get, post}, Json};
use uuid::Uuid;
use crate::models::session::LabSession;

pub fn sessions_routes() -> Router {
    Router::new()
        .route("/sessions", get(list_sessions))
        .route("/sessions/start/:lab_id", post(start_session))
        .route("/sessions/stop/:session_id", post(stop_session))
}

async fn list_sessions() -> Json<Vec<LabSession>> {
    Json(vec![]) // Mock for now
}

async fn start_session(axum::extract::Path(lab_id): axum::extract::Path<String>) -> Json<LabSession> {
    let session = LabSession {
        session_id: Uuid::new_v4(),
        user_id: "user-123".into(),
        lab_id,
        container_id: "mock-container".into(),
        status: "running".into(),
        webshell_url: "ws://localhost:3000/webshell/mock".into(),
    };

    Json(session)
}

async fn stop_session(axum::extract::Path(session_id): axum::extract::Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "session_id": session_id,
        "status": "stopped"
    }))
}
