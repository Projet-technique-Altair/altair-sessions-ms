use axum::{Router, routing::get, Json};
use serde_json::json;

use crate::state::AppState;

pub fn health_routes() -> Router<AppState> {
    Router::new().route("/", get(health))
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}
