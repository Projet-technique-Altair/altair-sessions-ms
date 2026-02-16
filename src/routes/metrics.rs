use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use crate::error::AppError;
use crate::models::api::{ApiMeta, ApiResponse};

#[derive(Serialize)]
pub struct Metrics {
    pub active_sessions: u32,
    pub spawn_attempts: u32,
}

pub async fn basic_metrics() -> Result<impl IntoResponse, AppError> {
    let metrics = Metrics {
        active_sessions: 1,
        spawn_attempts: 3,
    };

    let meta = ApiMeta {
        request_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let response = ApiResponse {
        success: true,
        data: metrics,
        meta,
    };

    Ok((StatusCode::OK, Json(response)))
}
