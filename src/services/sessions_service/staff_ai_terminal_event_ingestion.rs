use sqlx::Row;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::staff_analysis::{TerminalEventsIngestRequest, TerminalEventsIngestResponse},
};

use super::SessionsService;

impl SessionsService {
    pub async fn ingest_terminal_events(
        &self,
        payload: TerminalEventsIngestRequest,
    ) -> Result<TerminalEventsIngestResponse, AppError> {
        if payload.events.is_empty() {
            return Ok(TerminalEventsIngestResponse { accepted_count: 0 });
        }

        let session = sqlx::query(
            r#"
            SELECT user_id, lab_id
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(payload.session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        let session_user_id: Uuid = session
            .try_get("user_id")
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let session_lab_id: Uuid = session
            .try_get("lab_id")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if session_user_id != payload.user_id || session_lab_id != payload.lab_id {
            return Err(AppError::BadRequest(
                "Terminal event payload does not match the session".into(),
            ));
        }

        if let Some(runtime_id) = payload.runtime_id {
            let runtime_matches = sqlx::query_scalar::<_, bool>(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM lab_session_runtimes
                    WHERE runtime_id = $1
                      AND session_id = $2
                )
                "#,
            )
            .bind(runtime_id)
            .bind(payload.session_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            if !runtime_matches {
                return Err(AppError::BadRequest(
                    "Terminal event runtime does not belong to the session".into(),
                ));
            }
        }

        let mut tx = self
            .db
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut accepted_count = 0usize;
        for event in payload.events {
            let command = event.command_redacted.trim();
            if command.is_empty() {
                continue;
            }

            let result = sqlx::query(
                r#"
                INSERT INTO lab_terminal_events (
                    event_id,
                    session_id,
                    runtime_id,
                    user_id,
                    lab_id,
                    occurred_at,
                    command_redacted,
                    exit_status
                )
                VALUES (
                    COALESCE($1, gen_random_uuid()),
                    $2,
                    $3,
                    $4,
                    $5,
                    $6,
                    $7,
                    $8
                )
                ON CONFLICT (event_id) DO NOTHING
                "#,
            )
            .bind(event.event_id)
            .bind(payload.session_id)
            .bind(payload.runtime_id)
            .bind(payload.user_id)
            .bind(payload.lab_id)
            .bind(event.occurred_at.naive_utc())
            .bind(command.chars().take(1000).collect::<String>())
            .bind(event.exit_status)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

            accepted_count += result.rows_affected() as usize;
        }

        if accepted_count > 0 {
            sqlx::query(
                r#"
                UPDATE lab_sessions
                SET last_activity_at = NOW()
                WHERE session_id = $1
                "#,
            )
            .bind(payload.session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(TerminalEventsIngestResponse { accepted_count })
    }
}
