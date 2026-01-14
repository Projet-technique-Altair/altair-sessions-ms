use reqwest::Client;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::session::{Session, SessionRow, SessionStatus},
};

#[derive(Clone)]
pub struct SessionsService {
    db: PgPool,
    client: Client,
    lab_api_url: String,
}

impl SessionsService {
    pub fn new(db: PgPool) -> Self {
        Self {
            db,
            client: Client::new(),
            lab_api_url: std::env::var("LAB_API_URL")
                .unwrap_or_else(|_| "http://localhost:8085".to_string()),
        }
    }

    pub async fn start_session(&self, user_id: Uuid, lab_id: Uuid) -> Result<Session, AppError> {
        let existing = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM lab_sessions
            WHERE user_id = $1
              AND lab_id = $2
              AND status IN ('created', 'running')
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        if existing > 0 {
            return Err(AppError::Conflict("Session already active".into()));
        }

        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            INSERT INTO lab_sessions (user_id, lab_id, status)
            VALUES ($1, $2, 'created')
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(lab_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let session = Session::try_from(row)?;

        let spawn_result = self
            .client
            .post(format!("{}/spawn", self.lab_api_url))
            .json(&serde_json::json!({ "lab_id": lab_id }))
            .send()
            .await;

        match spawn_result {
            Ok(resp) => {
                let spawn: SpawnResponse = resp.json().await.map_err(|_| {
                    AppError::Internal("Invalid response from lab-api-service".into())
                })?;

                Self::validate_transition(session.status, SessionStatus::Running)?;

                sqlx::query(
                    r#"
                    UPDATE lab_sessions
                    SET status = 'running',
                        container_id = $1,
                        webshell_url = $2
                    WHERE session_id = $3
                    "#,
                )
                .bind(&spawn.container_id)
                .bind(&spawn.webshell_url)
                .bind(session.session_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            }
            Err(_) => {
                Self::validate_transition(session.status, SessionStatus::Error)?;

                sqlx::query(
                    r#"
                    UPDATE lab_sessions
                    SET status = 'error'
                    WHERE session_id = $1
                    "#,
                )
                .bind(session.session_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Internal(e.to_string()))?;
            }
        }

        self.get_session_by_id(session.session_id).await
    }

    pub async fn stop_session(&self, session_id: Uuid) -> Result<(), AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        let session = Session::try_from(row)?;

        if Self::is_terminal(session.status) {
            return Ok(());
        }

        if session.status == SessionStatus::Created {
            return Err(AppError::Conflict(
                "Cannot stop a session that has not started".into(),
            ));
        }

        Self::validate_transition(session.status, SessionStatus::Stopped)?;

        self.client
            .post(format!("{}/spawn/stop", self.lab_api_url))
            .json(&serde_json::json!({
                "container_id": session.container_id
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Stop call failed: {e}")))?;

        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET status = 'stopped'
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    pub async fn get_session_by_id(&self, session_id: Uuid) -> Result<Session, AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        Session::try_from(row)
    }

    pub async fn get_sessions_by_lab(&self, lab_id: Uuid) -> Result<Vec<Session>, AppError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE lab_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(lab_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        rows.into_iter().map(Session::try_from).collect()
    }

    pub async fn get_sessions_by_user(&self, user_id: Uuid) -> Result<Vec<Session>, AppError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        rows.into_iter().map(Session::try_from).collect()
    }

    fn validate_transition(from: SessionStatus, to: SessionStatus) -> Result<(), AppError> {
        use SessionStatus::*;

        let allowed = matches!(
            (from, to),
            (Created, Running)
                | (Created, Error)
                | (Running, Stopped)
                | (Running, Expired)
                | (Running, Error)
        );

        if allowed {
            Ok(())
        } else {
            Err(AppError::Conflict(format!(
                "Invalid session state transition: {:?} -> {:?}",
                from, to
            )))
        }
    }

    fn is_terminal(status: SessionStatus) -> bool {
        matches!(
            status,
            SessionStatus::Stopped | SessionStatus::Expired | SessionStatus::Error
        )
    }

    pub async fn expire_session(&self, session_id: Uuid) -> Result<(), AppError> {
        let row = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|_| AppError::NotFound("Session not found".into()))?;

        let session = Session::try_from(row)?;

        if Self::is_terminal(session.status) || session.status != SessionStatus::Running {
            return Ok(());
        }

        Self::validate_transition(session.status, SessionStatus::Expired)?;

        let _ = self
            .client
            .post(format!("{}/spawn/stop", self.lab_api_url))
            .json(&serde_json::json!({
                "container_id": session.container_id
            }))
            .send()
            .await;

        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET status = 'expired',
                expires_at = NOW()
            WHERE session_id = $1
            "#,
        )
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }

    pub async fn expire_all_expired_sessions(&self) -> Result<usize, AppError> {
        let rows = sqlx::query_as::<_, SessionRow>(
            r#"
            SELECT *
            FROM lab_sessions
            WHERE status = 'running'
              AND created_at + INTERVAL '2 hours' < NOW()
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let mut expired_count = 0;

        for row in rows {
            let session = Session::try_from(row)?;

            if session.status != SessionStatus::Running {
                continue;
            }

            if self.expire_session(session.session_id).await.is_ok() {
                expired_count += 1;
            }
        }

        Ok(expired_count)
    }
}

#[derive(Deserialize)]
struct SpawnResponse {
    container_id: String,
    webshell_url: String,
}
