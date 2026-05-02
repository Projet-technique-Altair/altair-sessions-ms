use uuid::Uuid;

use crate::error::AppError;

use super::SessionsService;

impl SessionsService {
    pub(super) async fn record_validation_event(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        lab_id: Uuid,
        step_number: i32,
        attempt_index: i32,
        answer: &str,
        is_correct: bool,
        validation_type: &str,
    ) -> Result<(), AppError> {
        let answer_redacted = answer.trim().chars().take(500).collect::<String>();

        sqlx::query(
            r#"
            INSERT INTO lab_validation_events (
                session_id,
                user_id,
                lab_id,
                step_number,
                attempt_index,
                answer_redacted,
                answer_hash,
                is_correct,
                validation_type
            )
            VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $8)
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(lab_id)
        .bind(step_number)
        .bind(attempt_index.max(1))
        .bind(answer_redacted)
        .bind(is_correct)
        .bind(validation_type)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    pub(super) async fn record_hint_event(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        lab_id: Uuid,
        step_number: i32,
        hint_id: Option<Uuid>,
        hint_number: i32,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO lab_hint_events (
                session_id,
                user_id,
                lab_id,
                step_number,
                hint_id,
                hint_number
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(session_id)
        .bind(user_id)
        .bind(lab_id)
        .bind(step_number)
        .bind(hint_id)
        .bind(hint_number)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }
}
