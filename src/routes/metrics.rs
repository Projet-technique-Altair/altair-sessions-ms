/**
 * @file metrics — basic metrics route handler.
 *
 * @remarks
 * Defines a lightweight metrics endpoint for the Sessions microservice.
 * This route returns a small operational payload wrapped in the standard
 * API response format.
 *
 * Responsibilities:
 *
 *  - Define the metrics payload exposed by the endpoint
 *  - Return basic session/runtime counters
 *  - Attach response metadata with request ID and timestamp
 *  - Wrap metrics data in the shared `ApiResponse` envelope
 *  - Return the response with an explicit HTTP status code
 *
 * Key characteristics:
 *
 *  - Uses a simple serializable `Metrics` structure
 *  - Provides placeholder counters for service observability
 *  - Uses the same metadata format as the rest of the API
 *  - Returns JSON through Axum response types
 *
 * This module provides the Sessions microservice with a minimal
 * observability endpoint that can later be connected to real runtime
 * metrics.
 *
 * @packageDocumentation
 */

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
