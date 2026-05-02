use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct TerminalEventsIngestRequest {
    pub session_id: Uuid,
    pub runtime_id: Option<Uuid>,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    #[serde(default)]
    pub events: Vec<TerminalEventPayload>,
}

#[derive(Debug, Deserialize)]
pub struct TerminalEventPayload {
    pub event_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
    pub command_redacted: String,
    pub exit_status: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct TerminalEventsIngestResponse {
    pub accepted_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StaffLabSummary {
    pub lab_id: Uuid,
    pub name: String,
    pub lab_delivery: String,
    pub objectives: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepeatedCommand {
    pub command: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignificantCommand {
    pub command: String,
    pub count: i64,
    pub failed_count: i64,
    pub first_seen_at: NaiveDateTime,
    pub last_seen_at: NaiveDateTime,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudentActivityMetrics {
    pub session_id: Option<Uuid>,
    pub student_user_id: Uuid,
    pub started_lab: bool,
    pub completed_lab: bool,
    pub current_step: Option<i32>,
    pub completed_steps: Vec<i32>,
    pub attempts_by_step: BTreeMap<String, i64>,
    pub validations_succeeded: i64,
    pub validations_failed: i64,
    pub hints_used: i64,
    pub terminal_used: bool,
    pub commands_count: i64,
    pub commands_failed_count: i64,
    pub commands_succeeded_count: i64,
    pub distinct_commands_count: i64,
    pub repeated_commands: Vec<RepeatedCommand>,
    pub significant_commands: Vec<SignificantCommand>,
    pub first_command_at: Option<NaiveDateTime>,
    pub last_command_at: Option<NaiveDateTime>,
    pub possible_blockers: Vec<String>,
    pub score: Option<i32>,
    pub max_score: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StudentActivityResponse {
    pub report_type: String,
    pub generated_at: DateTime<Utc>,
    pub lab: StaffLabSummary,
    pub metrics: StudentActivityMetrics,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabStaffAnalytics {
    pub lab: StaffLabSummary,
    pub generated_at: DateTime<Utc>,
    pub sessions_count: i64,
    pub distinct_students_count: i64,
    pub started_count: i64,
    pub completed_count: i64,
    pub terminal_sessions_count: i64,
    pub commands_count: i64,
    pub commands_failed_count: i64,
    pub validations_succeeded: i64,
    pub validations_failed: i64,
    pub hints_used: i64,
    pub common_blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupActivityMetrics {
    pub group_id: Uuid,
    pub students_count: i64,
    pub started_count: i64,
    pub completed_count: i64,
    pub terminal_sessions_count: i64,
    pub commands_count: i64,
    pub commands_failed_count: i64,
    pub validations_succeeded: i64,
    pub validations_failed: i64,
    pub hints_used: i64,
    pub common_blockers: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupActivityResponse {
    pub report_type: String,
    pub generated_at: DateTime<Utc>,
    pub lab: StaffLabSummary,
    pub metrics: GroupActivityMetrics,
    pub students: Vec<StudentActivityMetrics>,
}

#[derive(Debug, Serialize)]
pub struct AiAnalysisFinalResponse<T>
where
    T: Serialize,
{
    pub report_type: String,
    pub generated_at: DateTime<Utc>,
    pub metrics: T,
    pub report: serde_json::Value,
}
