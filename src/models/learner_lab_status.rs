use crate::error::AppError;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Product-facing learner state for a lab, separate from low-level runtime session states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LearnerLabStatusKind {
    Todo,
    InProgress,
    Finished,
}

/// Raw SQL row used before converting the string status into the typed enum above.
#[derive(Debug, Clone, FromRow)]
pub struct LearnerLabStatusRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    pub status: String,
    pub followed_at: NaiveDateTime,
    pub started_at: Option<NaiveDateTime>,
    pub finished_at: Option<NaiveDateTime>,
    pub last_activity_at: NaiveDateTime,
    pub last_session_id: Option<Uuid>,
}

/// Typed learner-lab relation exposed to the rest of the service layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerLabStatus {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    pub status: LearnerLabStatusKind,
    pub followed_at: NaiveDateTime,
    pub started_at: Option<NaiveDateTime>,
    pub finished_at: Option<NaiveDateTime>,
    pub last_activity_at: NaiveDateTime,
    pub last_session_id: Option<Uuid>,
}

impl TryFrom<LearnerLabStatusRow> for LearnerLabStatus {
    type Error = AppError;

    fn try_from(row: LearnerLabStatusRow) -> Result<Self, Self::Error> {
        // Keep DB values strict so invalid manual writes fail fast instead of leaking downstream.
        let status = match row.status.as_str() {
            "todo" => LearnerLabStatusKind::Todo,
            "in_progress" => LearnerLabStatusKind::InProgress,
            "finished" => LearnerLabStatusKind::Finished,
            other => {
                return Err(AppError::Internal(format!(
                    "Invalid learner_lab_status value in DB: {other}"
                )))
            }
        };

        Ok(Self {
            id: row.id,
            user_id: row.user_id,
            lab_id: row.lab_id,
            status,
            followed_at: row.followed_at,
            started_at: row.started_at,
            finished_at: row.finished_at,
            last_activity_at: row.last_activity_at,
            last_session_id: row.last_session_id,
        })
    }
}

/// Dashboard payload enriched with lab metadata and learner-specific progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnerDashboardLab {
    pub lab_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub difficulty: Option<String>,
    pub category: Option<String>,
    pub visibility: Option<String>,
    pub lab_delivery: Option<String>,
    pub estimated_duration: Option<String>,
    pub template_path: Option<String>,
    pub status: LearnerLabStatusKind,
    pub started_at: Option<NaiveDateTime>,
    pub finished_at: Option<NaiveDateTime>,
    pub last_activity_at: NaiveDateTime,
    pub progress: i32,
}
