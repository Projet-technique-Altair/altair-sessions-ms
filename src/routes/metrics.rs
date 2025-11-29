use axum::{Router, routing::get, Json};
use serde_json::json;

use crate::state::AppState;

pub fn metrics_routes() -> Router<AppState> {
    Router::new().route("/", get(metrics))
}

async fn metrics() -> Json<serde_json::Value> {
    Json(json!({
        "uptime": 12345,
        "requests_total": 7,
        "service": "sessions-ms"
    }))
}
