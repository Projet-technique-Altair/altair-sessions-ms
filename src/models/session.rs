/**
 * @file sessions — session domain models.
 *
 * @remarks
 * Defines the core data structures used to represent lab sessions
 * inside the Sessions microservice, including database rows, API-facing
 * session models, status values, and request payloads for session actions.
 *
 * Responsibilities:
 *
 *  - Represent session lifecycle states
 *  - Map raw database rows into typed domain models
 *  - Convert database status strings into `SessionStatus` values
 *  - Define the public session structure returned by the API
 *  - Define request payloads for step validation and hint requests
 *  - Preserve transitional runtime fields used by lab execution flows
 *
 * Key characteristics:
 *
 *  - Separates raw SQL rows from serialized API models
 *  - Uses UUIDs for users, labs, sessions, and runtimes
 *  - Uses explicit session statuses for lifecycle tracking
 *  - Validates database status values during conversion
 *  - Keeps compatibility fields for the LAB-WEB runtime transition
 *
 * This module acts as the domain boundary between persisted session
 * records and the typed models consumed by routes and services.
 *
 * @packageDocumentation
 */

use crate::error::AppError;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionStatus {
    Created,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, FromRow)]
pub struct SessionRow {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    pub current_runtime_id: Option<Uuid>,

    pub status: String,
    pub container_id: Option<String>,
    pub runtime_kind: Option<String>,
    pub webshell_url: Option<String>,
    // app_url remains stored temporarily for backend compatibility while the
    // LAB-WEB bootstrap-tab flow fully replaces the older direct-open contract.
    pub app_url: Option<String>,
    pub expires_at: Option<NaiveDateTime>,

    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
    pub last_activity_at: NaiveDateTime,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    pub current_runtime_id: Option<Uuid>,

    pub status: SessionStatus,
    pub container_id: Option<String>,
    pub runtime_kind: Option<String>,
    pub webshell_url: Option<String>,
    // Kept for transitional compatibility with backend callers; the frontend no
    // longer depends on app_url in the current LAB-WEB flow.
    pub app_url: Option<String>,
    pub expires_at: Option<NaiveDateTime>,

    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
    pub last_activity_at: NaiveDateTime,
}

impl TryFrom<SessionRow> for Session {
    type Error = AppError;

    fn try_from(row: SessionRow) -> Result<Self, Self::Error> {
        let status = match row.status.as_str() {
            "created" => SessionStatus::Created,
            "in_progress" => SessionStatus::InProgress,
            "completed" => SessionStatus::Completed,
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
            current_runtime_id: row.current_runtime_id,
            container_id: row.container_id,
            status,
            runtime_kind: row.runtime_kind,
            webshell_url: row.webshell_url,
            app_url: row.app_url,
            expires_at: row.expires_at,
            created_at: row.created_at,
            completed_at: row.completed_at,
            last_activity_at: row.last_activity_at,
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
