/**
 * @file lab_progress — lab progress models.
 *
 * @remarks
 * Defines the structures used to represent learner progress inside
 * a lab session, from raw persisted progress data to the API-facing
 * payload returned to the frontend.
 *
 * Responsibilities:
 *
 *  - Represent persisted lab progress rows from PostgreSQL
 *  - Expose frontend-ready lab progress data
 *  - Convert JSON progress aggregates into typed API fields
 *  - Compute the total number of attempts across all steps
 *  - Normalize used hints into a string list
 *  - Attach computed runtime duration to the progress payload
 *
 * Key characteristics:
 *
 *  - Separates raw database representation from API output
 *  - Stores step completion and scoring information
 *  - Uses JSON fields for flexible hint and attempt tracking
 *  - Converts persisted aggregates into simpler frontend fields
 *  - Includes elapsed time as a computed value in seconds
 *
 * This module acts as the conversion layer between stored lab progress
 * data and the simplified progress state consumed by the frontend.
 *
 * @packageDocumentation
 */

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;


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
    /// Builds the API payload from persisted aggregates and computed runtime time.
    pub fn from_row(row: LabProgressRow, time_elapsed: i64) -> Self {
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
