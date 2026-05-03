/**
 * @file error — application error handling.
 *
 * @remarks
 * Defines the domain-level error type used by the Sessions
 * microservice and converts application errors into standardized
 * HTTP JSON responses.
 *
 * Responsibilities:
 *
 *  - Represent common application errors with `AppError`
 *  - Associate each error variant with the appropriate HTTP status code
 *  - Convert errors into Axum responses through `IntoResponse`
 *  - Return consistent API error payloads
 *  - Generate response metadata for failed requests
 *
 * Key characteristics:
 *
 *  - Uses `thiserror` for readable error definitions
 *  - Maps domain errors to explicit API error codes
 *  - Preserves the shared `ApiErrorResponse` format
 *  - Includes request metadata with UUID and timestamp
 *  - Supports session-specific errors such as wrong answers
 *
 * This module centralizes error-to-response conversion so route
 * handlers and services can return typed errors while keeping API
 * responses consistent across the microservice.
 *
 * @packageDocumentation
 */

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::models::api::{ApiError, ApiErrorResponse, ApiMeta};

#[allow(dead_code)]
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Resource not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Wrong answer")]
    WrongAnswer { attempts: i32 },
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "RESOURCE_NOT_FOUND", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg),
            AppError::WrongAnswer { attempts } => (
                StatusCode::BAD_REQUEST,
                "WRONG_ANSWER",
                format!("Wrong answer (attempts: {attempts})"),
            ),
        };

        let error = ApiError {
            code: error_code.to_string(),
            message,
            details: None,
        };

        let meta = ApiMeta {
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        let body = ApiErrorResponse {
            success: false,
            error,
            meta,
        };

        (status, Json(body)).into_response()
    }
}
