/**
 * @file extractor — caller identity extraction.
 *
 * @remarks
 * Provides helper logic used by route handlers to extract the authenticated
 * caller from gateway-injected HTTP headers and normalize the caller role
 * into a single service-level role.
 *
 * Responsibilities:
 *
 *  - Read the caller user ID from `x-altair-user-id`
 *  - Parse the caller identifier as a UUID
 *  - Read raw roles from `x-altair-roles`
 *  - Normalize multiple raw roles into one effective role
 *  - Reject requests with missing identity data
 *  - Reject callers without a valid supported role
 *
 * Key characteristics:
 *
 *  - Trusts identity headers provided by the gateway layer
 *  - Uses `admin` as the highest-priority role
 *  - Falls back to `creator`, then `learner`
 *  - Returns a compact `Caller` structure for service authorization
 *  - Converts invalid authentication context into typed `AppError` values
 *
 * This module centralizes caller extraction so route handlers can rely on
 * a consistent identity and authorization context.
 *
 * @packageDocumentation
 */

use crate::error::AppError;
use axum::http::HeaderMap;
use uuid::Uuid;

#[derive(Debug)]
pub struct Caller {
    pub user_id: Uuid,
    pub roles: Vec<String>,
}

pub fn extract_caller(headers: &HeaderMap) -> Result<Caller, AppError> {
    let user_id = headers
        .get("x-altair-user-id")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| AppError::Unauthorized("Missing caller identity".to_string()))?;

    let raw_roles: Vec<String> = headers
        .get("x-altair-roles")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').map(|r| r.to_string()).collect())
        .unwrap_or_default();

    let roles = normalize_roles(&raw_roles)?;

    Ok(Caller { user_id, roles })
}

fn normalize_roles(raw_roles: &[String]) -> Result<Vec<String>, AppError> {
    let has_admin = raw_roles.iter().any(|r| r == "admin");
    let has_creator = raw_roles.iter().any(|r| r == "creator");
    let has_learner = raw_roles.iter().any(|r| r == "learner");

    let role = if has_admin {
        "admin"
    } else if has_creator {
        "creator"
    } else if has_learner {
        "learner"
    } else {
        return Err(AppError::Forbidden("No valid role".into()));
    };

    Ok(vec![role.to_string()])
}
