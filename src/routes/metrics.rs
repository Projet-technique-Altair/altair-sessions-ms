use axum::Json;
use serde_json::json;

pub async fn basic_metrics() -> Json<serde_json::Value> {
    Json(json!({
        "active_sessions": 1,
        "spawn_attempts": 3
    }))
}
