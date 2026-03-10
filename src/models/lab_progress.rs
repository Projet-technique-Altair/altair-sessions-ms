use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// Représentation DB directe (sqlx)
#[allow(dead_code)]
#[derive(Debug, Clone, FromRow)]
pub struct LabProgressRow {
    pub progress_id: Uuid,
    pub session_id: Uuid,

    pub current_step: i32,
    pub completed_steps: Vec<i32>,

    pub hints_used: Value,
    pub attempts_per_step: Value,

    pub score: i32,
    pub max_score: i32,

    pub created_at: NaiveDateTime,
}

/// Représentation API exposée au frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabProgress {
    pub session_id: Uuid,

    pub current_step: i32,
    pub completed_steps: Vec<i32>,
    pub hints_used: Vec<String>,

    pub attempts: i32,
    pub score: i32,
    pub max_score: i32,

    pub time_elapsed: i64, // en secondes
}

impl LabProgress {
    /// Calcule attempts totales depuis attempts_per_step
    pub fn from_row(row: LabProgressRow, session_created_at: NaiveDateTime) -> Self {
        let attempts = row
            .attempts_per_step
            .as_object()
            .map(|m| m.values().filter_map(|v| v.as_i64()).sum::<i64>())
            .unwrap_or(0) as i32;

        let hints_used = row
            .hints_used
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let now = chrono::Utc::now().naive_utc();
        let time_elapsed = (now - session_created_at).num_seconds();

        Self {
            session_id: row.session_id,
            current_step: row.current_step,
            completed_steps: row.completed_steps,
            hints_used,
            attempts,
            score: row.score,
            max_score: row.max_score,
            time_elapsed,
        }
    }
}
