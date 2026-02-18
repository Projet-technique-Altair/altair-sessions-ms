use uuid::Uuid;
use axum::http::HeaderMap;
use crate::error::AppError;


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


    let roles = headers
        .get("x-altair-roles")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').map(|r| r.to_string()).collect())
        .unwrap_or_default();

    Ok(Caller { user_id, roles })
}
