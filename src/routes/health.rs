/**
 * @file health — health check route handler.
 *
 * @remarks
 * Defines the health endpoint used to verify that the Sessions
 * microservice is running and able to return a standard API response.
 *
 * Responsibilities:
 *
 *  - Expose a lightweight service health check
 *  - Return an HTTP 200 status when the service is reachable
 *  - Build a standardized success response
 *  - Attach response metadata with request ID and timestamp
 *  - Avoid performing expensive dependency checks
 *
 * Key characteristics:
 *
 *  - Uses the shared `ApiResponse` envelope
 *  - Returns an empty success payload
 *  - Generates fresh metadata for each request
 *  - Can be used by local tools, orchestrators, or monitoring systems
 *
 * This module provides a minimal readiness-style endpoint for confirming
 * that the Sessions microservice HTTP layer is alive.
 *
 * @packageDocumentation
 */

use axum::{http::StatusCode, response::IntoResponse, Json};

use crate::models::api::{ApiMeta, ApiResponse};

pub async fn health() -> impl IntoResponse {
    let meta = ApiMeta {
        request_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let response = ApiResponse {
        success: true,
        data: None::<()>,
        meta,
    };

    (StatusCode::OK, Json(response))
}
