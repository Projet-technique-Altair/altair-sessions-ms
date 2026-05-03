/**
 * @file api — standardized API response models.
 *
 * @remarks
 * Defines the shared response structures used by the Sessions
 * microservice to return consistent JSON payloads for both successful
 * and failed requests.
 *
 * Responsibilities:
 *
 *  - Define common response metadata
 *  - Define standardized API error payloads
 *  - Define generic success response envelopes
 *  - Define error response envelopes
 *  - Generate request identifiers and timestamps
 *  - Provide helper constructors for success and error responses
 *
 * Key characteristics:
 *
 *  - Uses a generic `ApiResponse<T>` for typed success payloads
 *  - Uses `ApiErrorResponse` for structured failure payloads
 *  - Attaches metadata to every API response
 *  - Generates UUID-based request identifiers
 *  - Uses UTC RFC3339 timestamps for traceability
 *
 * This module keeps the public API response format predictable and
 * consistent across all Sessions microservice routes.
 *
 * @packageDocumentation
 */

use serde::Serialize;

#[derive(Serialize)]
pub struct ApiMeta {
    pub request_id: String,
    pub timestamp: String,
}

#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Serialize)]
pub struct ApiErrorResponse {
    pub success: bool,
    pub error: ApiError,
    pub meta: ApiMeta,
}

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
    pub meta: ApiMeta,
}

impl ApiMeta {
    pub fn new() -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data,
            meta: ApiMeta::new(),
        }
    }
}

#[allow(dead_code)]
impl ApiErrorResponse {
    pub fn from_error(error: ApiError) -> Self {
        Self {
            success: false,
            error,
            meta: ApiMeta::new(),
        }
    }
}
