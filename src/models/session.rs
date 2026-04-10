use crate::error::AppError;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionStatus {
    Created,
    Running,
    Stopped,
    Expired,
    Error,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,

    pub container_id: Option<String>,
    pub status: String, // DB value (lowercase string)
    pub runtime_kind: Option<String>,
    pub webshell_url: Option<String>,
    // app_url remains stored temporarily for backend compatibility while the
    // LAB-WEB bootstrap-tab flow fully replaces the older direct-open contract.
    pub app_url: Option<String>,

    pub created_at: chrono::NaiveDateTime,
    pub expires_at: Option<chrono::NaiveDateTime>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,

    pub container_id: Option<String>,
    pub status: SessionStatus,
    pub runtime_kind: Option<String>,
    pub webshell_url: Option<String>,
    // Kept for transitional compatibility with backend callers; the frontend no
    // longer depends on app_url in the current LAB-WEB flow.
    pub app_url: Option<String>,

    pub created_at: chrono::NaiveDateTime,
    pub expires_at: Option<chrono::NaiveDateTime>,
}

impl TryFrom<SessionRow> for Session {
    type Error = AppError;

    fn try_from(row: SessionRow) -> Result<Self, Self::Error> {
        let status = match row.status.as_str() {
            "created" => SessionStatus::Created,
            "running" => SessionStatus::Running,
            "stopped" => SessionStatus::Stopped,
            "expired" => SessionStatus::Expired,
            "error" => SessionStatus::Error,
            other => {
                return Err(AppError::Internal(format!(
                    "Invalid session status in DB: {other}"
                )))
            }
        };

        Ok(Session {
            session_id: row.session_id,
            user_id: row.user_id,
            lab_id: row.lab_id,
            container_id: row.container_id,
            status,
            runtime_kind: row.runtime_kind,
            webshell_url: row.webshell_url,
            app_url: row.app_url,
            created_at: row.created_at,
            expires_at: row.expires_at,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct ValidateStepRequest {
    pub step_number: i32,
    pub user_answer: String,
}

#[derive(Debug, Deserialize)]
pub struct RequestHintRequest {
    pub step_number: i32,
    pub hint_number: i32,
}
