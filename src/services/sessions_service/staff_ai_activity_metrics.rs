use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{NaiveDateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        lab_progress::LabProgressRow,
        session::SessionRow,
        staff_analysis::{
            GroupActivityMetrics, GroupActivityResponse, LabStaffAnalytics, RepeatedCommand,
            SignificantCommand, StaffLabSummary, StudentActivityMetrics, StudentActivityResponse,
        },
    },
};

use super::{LabOverview, SessionsService, SESSION_SELECT};

#[derive(Debug, Clone, sqlx::FromRow)]
struct TerminalEventRow {
    command_redacted: String,
    exit_status: Option<i32>,
    occurred_at: NaiveDateTime,
}

#[derive(Debug, Clone)]
struct CommandAggregate {
    command: String,
    count: i64,
    failed_count: i64,
    first_seen_at: NaiveDateTime,
    last_seen_at: NaiveDateTime,
    first_index: usize,
}

impl SessionsService {
    pub async fn get_staff_lab_analytics(
        &self,
        requester_user_id: Uuid,
        is_admin: bool,
        lab_id: Uuid,
    ) -> Result<LabStaffAnalytics, AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;
        self.ensure_staff_can_access_lab(requester_user_id, is_admin, &lab)?;
        self.ensure_terminal_lab(&lab)?;

        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*)::BIGINT AS sessions_count,
                COUNT(DISTINCT user_id)::BIGINT AS distinct_students_count,
                COUNT(*) FILTER (WHERE status IN ('created', 'in_progress', 'completed'))::BIGINT AS started_count,
                COUNT(*) FILTER (WHERE status = 'completed')::BIGINT AS completed_count
            FROM lab_sessions
            WHERE lab_id = $1
            "#,
        )
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let terminal_sessions_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(DISTINCT session_id)::BIGINT
            FROM lab_terminal_events
            WHERE lab_id = $1
            "#,
        )
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let command_counts = sqlx::query(
            r#"
            SELECT
                COUNT(*)::BIGINT AS commands_count,
                COUNT(*) FILTER (WHERE exit_status IS NOT NULL AND exit_status <> 0)::BIGINT AS commands_failed_count
            FROM lab_terminal_events
            WHERE lab_id = $1
            "#,
        )
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let validation_counts = self.validation_counts_for_lab(lab_id).await?;
        let hints_used = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::BIGINT
            FROM lab_hint_events
            WHERE lab_id = $1
            "#,
        )
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(LabStaffAnalytics {
            lab: self.staff_lab_summary(&lab),
            generated_at: Utc::now(),
            sessions_count: row.try_get("sessions_count").unwrap_or(0),
            distinct_students_count: row.try_get("distinct_students_count").unwrap_or(0),
            started_count: row.try_get("started_count").unwrap_or(0),
            completed_count: row.try_get("completed_count").unwrap_or(0),
            terminal_sessions_count,
            commands_count: command_counts.try_get("commands_count").unwrap_or(0),
            commands_failed_count: command_counts.try_get("commands_failed_count").unwrap_or(0),
            validations_succeeded: validation_counts.0,
            validations_failed: validation_counts.1,
            hints_used,
            common_blockers: self.common_blockers_for_lab(lab_id).await?,
        })
    }

    pub async fn get_staff_student_activity(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        is_admin: bool,
        lab_id: Uuid,
        student_user_id: Uuid,
    ) -> Result<StudentActivityResponse, AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;
        self.ensure_terminal_lab(&lab)?;

        if !is_admin && lab.creator_id != requester_user_id {
            let allowed = self
                .student_is_in_creator_group_for_lab(
                    requester_user_id,
                    roles_header,
                    lab_id,
                    student_user_id,
                )
                .await?;
            if !allowed {
                return Err(AppError::Forbidden(
                    "You are not allowed to inspect this student activity".into(),
                ));
            }
        }

        let total_steps = self.fetch_lab_steps_count(lab_id).await.unwrap_or(0);
        let metrics = self
            .build_student_activity_metrics(lab_id, student_user_id, total_steps)
            .await?;

        Ok(StudentActivityResponse {
            report_type: "individual_student_activity".to_string(),
            generated_at: Utc::now(),
            lab: self.staff_lab_summary(&lab),
            metrics,
        })
    }

    pub async fn get_staff_group_activity(
        &self,
        requester_user_id: Uuid,
        roles_header: &str,
        is_admin: bool,
        lab_id: Uuid,
        group_id: Uuid,
    ) -> Result<GroupActivityResponse, AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;
        self.ensure_terminal_lab(&lab)?;
        self.ensure_group_and_lab_allowed(
            requester_user_id,
            roles_header,
            is_admin,
            lab_id,
            &lab,
            group_id,
        )
        .await?;

        let members = self
            .fetch_group_members(requester_user_id, roles_header, group_id)
            .await?;
        let total_steps = self.fetch_lab_steps_count(lab_id).await.unwrap_or(0);
        let mut students = Vec::with_capacity(members.len());

        for member in members {
            students.push(
                self.build_student_activity_metrics(lab_id, member.user_id, total_steps)
                    .await?,
            );
        }

        let metrics = Self::aggregate_group_metrics(group_id, &students);

        Ok(GroupActivityResponse {
            report_type: "group_activity".to_string(),
            generated_at: Utc::now(),
            lab: self.staff_lab_summary(&lab),
            metrics,
            students,
        })
    }

    pub async fn get_staff_common_blockers(
        &self,
        requester_user_id: Uuid,
        is_admin: bool,
        lab_id: Uuid,
    ) -> Result<Vec<String>, AppError> {
        let lab = self.fetch_lab_overview(lab_id).await?;
        self.ensure_staff_can_access_lab(requester_user_id, is_admin, &lab)?;
        self.ensure_terminal_lab(&lab)?;
        self.common_blockers_for_lab(lab_id).await
    }

    pub(super) fn ensure_staff_can_access_lab(
        &self,
        requester_user_id: Uuid,
        is_admin: bool,
        lab: &LabOverview,
    ) -> Result<(), AppError> {
        if is_admin || lab.creator_id == requester_user_id {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "You are not allowed to inspect this lab".into(),
            ))
        }
    }

    fn ensure_terminal_lab(&self, lab: &LabOverview) -> Result<(), AppError> {
        if lab.lab_delivery.as_deref() == Some("terminal") {
            Ok(())
        } else {
            Err(AppError::UnsupportedLabType)
        }
    }

    fn staff_lab_summary(&self, lab: &LabOverview) -> StaffLabSummary {
        StaffLabSummary {
            lab_id: lab.lab_id,
            name: lab.name.clone(),
            lab_delivery: lab
                .lab_delivery
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            objectives: Self::parse_objectives(lab.objectives.as_deref()),
        }
    }

    fn parse_objectives(objectives: Option<&str>) -> Vec<String> {
        objectives
            .unwrap_or_default()
            .split(['\n', ';'])
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    async fn build_student_activity_metrics(
        &self,
        lab_id: Uuid,
        student_user_id: Uuid,
        total_steps: i32,
    ) -> Result<StudentActivityMetrics, AppError> {
        let sql = format!(
            "{SESSION_SELECT}
            WHERE s.lab_id = $1
              AND s.user_id = $2
            ORDER BY s.created_at DESC
            LIMIT 1"
        );

        let session_row = sqlx::query_as::<_, SessionRow>(&sql)
            .bind(lab_id)
            .bind(student_user_id)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let Some(session) = session_row else {
            return Ok(StudentActivityMetrics {
                session_id: None,
                student_user_id,
                started_lab: false,
                completed_lab: false,
                current_step: None,
                completed_steps: Vec::new(),
                attempts_by_step: BTreeMap::new(),
                validations_succeeded: 0,
                validations_failed: 0,
                hints_used: 0,
                terminal_used: false,
                commands_count: 0,
                commands_failed_count: 0,
                commands_succeeded_count: 0,
                distinct_commands_count: 0,
                repeated_commands: Vec::new(),
                significant_commands: Vec::new(),
                first_command_at: None,
                last_command_at: None,
                possible_blockers: vec!["Aucune session exploitable pour ce lab".to_string()],
                score: None,
                max_score: None,
            });
        };

        let progress = sqlx::query_as::<_, LabProgressRow>(
            r#"
            SELECT *
            FROM lab_progress
            WHERE session_id = $1
            "#,
        )
        .bind(session.session_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let events = self.load_terminal_events(session.session_id).await?;
        let (repeated_commands, significant_commands) = Self::analyze_significant_commands(&events);

        let validations = self
            .validation_counts_for_session(session.session_id)
            .await?;
        let hint_event_count = self.hint_count_for_session(session.session_id).await?;

        let attempts_by_step = progress
            .as_ref()
            .map(|p| Self::attempts_from_progress(&p.attempts_per_step))
            .unwrap_or_default();

        let progress_hints_count = progress
            .as_ref()
            .and_then(|p| p.hints_used.as_array())
            .map(|items| items.len() as i64)
            .unwrap_or(0);

        let completed_steps = progress
            .as_ref()
            .map(|p| p.completed_steps.clone())
            .unwrap_or_default();

        let completed_from_progress =
            total_steps > 0 && completed_steps.len() as i32 >= total_steps;

        let commands_count = events.len() as i64;
        let commands_failed_count = events
            .iter()
            .filter(|event| event.exit_status.is_some_and(|status| status != 0))
            .count() as i64;
        let commands_succeeded_count = events
            .iter()
            .filter(|event| event.exit_status == Some(0))
            .count() as i64;
        let distinct_commands_count = events
            .iter()
            .map(|event| event.command_redacted.as_str())
            .collect::<HashSet<_>>()
            .len() as i64;

        let first_command_at = events.first().map(|event| event.occurred_at);
        let last_command_at = events.last().map(|event| event.occurred_at);
        let hints_used = hint_event_count.max(progress_hints_count);

        let mut possible_blockers = Self::detect_student_blockers(
            commands_count,
            commands_failed_count,
            &repeated_commands,
            validations.0,
            validations.1,
            hints_used,
            completed_steps.len(),
        );

        if possible_blockers.is_empty() && commands_count == 0 {
            possible_blockers.push("Aucune commande terminal collectee".to_string());
        }

        Ok(StudentActivityMetrics {
            session_id: Some(session.session_id),
            student_user_id,
            started_lab: true,
            completed_lab: session.status == "completed" || completed_from_progress,
            current_step: progress.as_ref().map(|p| p.current_step),
            completed_steps,
            attempts_by_step,
            validations_succeeded: validations.0,
            validations_failed: validations.1,
            hints_used,
            terminal_used: commands_count > 0,
            commands_count,
            commands_failed_count,
            commands_succeeded_count,
            distinct_commands_count,
            repeated_commands,
            significant_commands,
            first_command_at,
            last_command_at,
            possible_blockers,
            score: progress.as_ref().map(|p| p.score),
            max_score: progress.as_ref().map(|p| p.max_score),
        })
    }

    fn attempts_from_progress(value: &serde_json::Value) -> BTreeMap<String, i64> {
        value
            .as_object()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|(key, value)| value.as_i64().map(|count| (key.clone(), count)))
                    .collect()
            })
            .unwrap_or_default()
    }

    async fn load_terminal_events(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<TerminalEventRow>, AppError> {
        sqlx::query_as::<_, TerminalEventRow>(
            r#"
            SELECT command_redacted, exit_status, occurred_at
            FROM lab_terminal_events
            WHERE session_id = $1
            ORDER BY occurred_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
    }

    fn analyze_significant_commands(
        events: &[TerminalEventRow],
    ) -> (Vec<RepeatedCommand>, Vec<SignificantCommand>) {
        let mut aggregates: HashMap<String, CommandAggregate> = HashMap::new();
        let mut chronological_commands = Vec::new();

        for (index, event) in events.iter().enumerate() {
            let command = event.command_redacted.trim();
            if Self::is_trivial_command(command) {
                continue;
            }

            chronological_commands.push((index, event));
            aggregates
                .entry(command.to_string())
                .and_modify(|agg| {
                    agg.count += 1;
                    if event.exit_status.is_some_and(|status| status != 0) {
                        agg.failed_count += 1;
                    }
                    agg.last_seen_at = event.occurred_at;
                })
                .or_insert_with(|| CommandAggregate {
                    command: command.to_string(),
                    count: 1,
                    failed_count: if event.exit_status.is_some_and(|status| status != 0) {
                        1
                    } else {
                        0
                    },
                    first_seen_at: event.occurred_at,
                    last_seen_at: event.occurred_at,
                    first_index: index,
                });
        }

        let mut repeated_commands = aggregates
            .values()
            .filter(|agg| agg.count >= 2)
            .map(|agg| RepeatedCommand {
                command: agg.command.clone(),
                count: agg.count,
            })
            .collect::<Vec<_>>();
        repeated_commands.sort_by(|a, b| b.count.cmp(&a.count).then(a.command.cmp(&b.command)));

        let mut candidate_importance: HashMap<String, (i32, String)> = HashMap::new();
        for agg in aggregates.values() {
            if agg.failed_count > 0 {
                candidate_importance
                    .entry(agg.command.clone())
                    .and_modify(|item| {
                        if item.0 < 3 {
                            *item = (3, "failed_command".to_string());
                        }
                    })
                    .or_insert_with(|| (3, "failed_command".to_string()));
            }
            if agg.count >= 2 {
                candidate_importance
                    .entry(agg.command.clone())
                    .and_modify(|item| {
                        if item.0 < 2 {
                            *item = (2, "repeated_command".to_string());
                        }
                    })
                    .or_insert_with(|| (2, "repeated_command".to_string()));
            }
        }

        let mut previous_command: Option<&str> = None;
        let mut repeated_block_len = 0;
        let mut consecutive_failures = 0;
        let mut seen = HashSet::new();

        for (_, event) in chronological_commands {
            let command = event.command_redacted.as_str();
            let is_same_as_previous = previous_command == Some(command);
            if is_same_as_previous {
                repeated_block_len += 1;
            } else {
                if !seen.contains(command)
                    && (repeated_block_len >= 2 || consecutive_failures >= 2)
                    && !candidate_importance.contains_key(command)
                {
                    candidate_importance
                        .insert(command.to_string(), (1, "new_attempt".to_string()));
                }
                repeated_block_len = 1;
            }

            if event.exit_status.is_some_and(|status| status != 0) {
                consecutive_failures += 1;
            } else {
                consecutive_failures = 0;
            }

            seen.insert(command.to_string());
            previous_command = Some(command);
        }

        let mut significant = candidate_importance
            .into_iter()
            .filter_map(|(command, (importance, reason))| {
                aggregates.get(&command).map(|agg| {
                    (
                        importance,
                        SignificantCommand {
                            command,
                            count: agg.count,
                            failed_count: agg.failed_count,
                            first_seen_at: agg.first_seen_at,
                            last_seen_at: agg.last_seen_at,
                            reason,
                        },
                        agg.first_index,
                    )
                })
            })
            .collect::<Vec<_>>();

        significant.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.count.cmp(&a.1.count))
                .then(a.2.cmp(&b.2))
        });

        (
            repeated_commands,
            significant
                .into_iter()
                .take(10)
                .map(|(_, command, _)| command)
                .collect(),
        )
    }

    fn is_trivial_command(command: &str) -> bool {
        matches!(
            command.trim(),
            "" | "clear" | "reset" | "history" | "exit" | "logout"
        )
    }

    fn detect_student_blockers(
        commands_count: i64,
        commands_failed_count: i64,
        repeated_commands: &[RepeatedCommand],
        validations_succeeded: i64,
        validations_failed: i64,
        hints_used: i64,
        completed_steps_count: usize,
    ) -> Vec<String> {
        let mut blockers = Vec::new();

        if commands_count > 0
            && commands_failed_count >= 5
            && commands_failed_count * 100 / commands_count.max(1) >= 35
        {
            blockers.push("Nombre eleve de commandes en echec".to_string());
        }

        if let Some(command) = repeated_commands.iter().find(|command| command.count >= 3) {
            blockers.push(format!(
                "Commande repetee plusieurs fois: {}",
                command.command
            ));
        }

        if commands_count > 0 && completed_steps_count == 0 {
            blockers.push("Activite terminal sans etape validee".to_string());
        }

        if validations_failed >= 3 && validations_succeeded == 0 {
            blockers.push("Validations en echec sans validation reussie".to_string());
        }

        if hints_used >= 3 {
            blockers.push("Nombreux hints utilises".to_string());
        }

        blockers
    }

    async fn validation_counts_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<(i64, i64), AppError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE is_correct = true)::BIGINT AS succeeded,
                COUNT(*) FILTER (WHERE is_correct = false)::BIGINT AS failed
            FROM lab_validation_events
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok((
            row.try_get("succeeded").unwrap_or(0),
            row.try_get("failed").unwrap_or(0),
        ))
    }

    async fn validation_counts_for_lab(&self, lab_id: Uuid) -> Result<(i64, i64), AppError> {
        let row = sqlx::query(
            r#"
            SELECT
                COUNT(*) FILTER (WHERE is_correct = true)::BIGINT AS succeeded,
                COUNT(*) FILTER (WHERE is_correct = false)::BIGINT AS failed
            FROM lab_validation_events
            WHERE lab_id = $1
            "#,
        )
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok((
            row.try_get("succeeded").unwrap_or(0),
            row.try_get("failed").unwrap_or(0),
        ))
    }

    async fn hint_count_for_session(&self, session_id: Uuid) -> Result<i64, AppError> {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::BIGINT
            FROM lab_hint_events
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
    }

    async fn common_blockers_for_lab(&self, lab_id: Uuid) -> Result<Vec<String>, AppError> {
        let failed_commands = sqlx::query(
            r#"
            SELECT command_redacted, COUNT(*)::BIGINT AS failed_count
            FROM lab_terminal_events
            WHERE lab_id = $1
              AND exit_status IS NOT NULL
              AND exit_status <> 0
              AND command_redacted <> ''
            GROUP BY command_redacted
            ORDER BY failed_count DESC, command_redacted ASC
            LIMIT 5
            "#,
        )
        .bind(lab_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let failed_steps = sqlx::query(
            r#"
            SELECT step_number, COUNT(*)::BIGINT AS failed_count
            FROM lab_validation_events
            WHERE lab_id = $1
              AND is_correct = false
            GROUP BY step_number
            ORDER BY failed_count DESC, step_number ASC
            LIMIT 5
            "#,
        )
        .bind(lab_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut blockers = Vec::new();
        for row in failed_commands {
            let command: String = row.try_get("command_redacted").unwrap_or_default();
            let count: i64 = row.try_get("failed_count").unwrap_or(0);
            if count > 0 {
                blockers.push(format!("Commande souvent en echec: {command} ({count})"));
            }
        }

        for row in failed_steps {
            let step_number: i32 = row.try_get("step_number").unwrap_or(0);
            let count: i64 = row.try_get("failed_count").unwrap_or(0);
            if count > 0 {
                blockers.push(format!(
                    "Etape {step_number} avec validations echouees frequentes ({count})"
                ));
            }
        }

        Ok(blockers)
    }

    fn aggregate_group_metrics(
        group_id: Uuid,
        students: &[StudentActivityMetrics],
    ) -> GroupActivityMetrics {
        let mut blockers_count: HashMap<String, i64> = HashMap::new();

        for student in students {
            for blocker in &student.possible_blockers {
                *blockers_count.entry(blocker.clone()).or_insert(0) += 1;
            }
        }

        let mut common_blockers = blockers_count.into_iter().collect::<Vec<_>>();
        common_blockers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        GroupActivityMetrics {
            group_id,
            students_count: students.len() as i64,
            started_count: students.iter().filter(|s| s.started_lab).count() as i64,
            completed_count: students.iter().filter(|s| s.completed_lab).count() as i64,
            terminal_sessions_count: students.iter().filter(|s| s.terminal_used).count() as i64,
            commands_count: students.iter().map(|s| s.commands_count).sum(),
            commands_failed_count: students.iter().map(|s| s.commands_failed_count).sum(),
            validations_succeeded: students.iter().map(|s| s.validations_succeeded).sum(),
            validations_failed: students.iter().map(|s| s.validations_failed).sum(),
            hints_used: students.iter().map(|s| s.hints_used).sum(),
            common_blockers: common_blockers
                .into_iter()
                .take(6)
                .map(|(blocker, count)| format!("{blocker} ({count})"))
                .collect(),
        }
    }
}
