use sqlx::{PgPool, Row};
use uuid::Uuid;
use reqwest::Client;
use serde::Deserialize;

use crate::{
    models::session::Session,
    error::AppError,
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

    /// GET /sessions
    pub async fn list_sessions(&self) -> Result<Vec<Session>, AppError> {
    let sessions = sqlx::query_as::<_, Session>(
        r#"
        SELECT
            session_id,
            user_id,
            lab_id,
            container_id,
            status,
            webshell_url,
            created_at,
            expires_at
        FROM lab_sessions
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(&self.db)
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(sessions)
}


    /// POST /sessions/start
    pub async fn start_session(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
    ) -> Result<Session, AppError> {
        // 1️⃣ Call lab-api-service
        let resp = self.client
            .post(format!("{}/spawn", self.lab_api_url))
            .json(&serde_json::json!({
                "lab_id": lab_id
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Spawn call failed: {e}")))?;

        let spawn: SpawnResponse = resp
            .json()
            .await
            .map_err(|_| AppError::Internal("Invalid response from lab-api-service".into()))?;

        // 2️⃣ Insert into DB
        let session = sqlx::query_as::<_, Session>(
            r#"
            INSERT INTO lab_sessions (
                user_id,
                lab_id,
                container_id,
                status,
                webshell_url
            )
            VALUES ($1, $2, $3, $4, $5)
            RETURNING
                session_id,
                user_id,
                lab_id,
                container_id,
                status,
                webshell_url,
                created_at,
                expires_at
            "#
        )
        .bind(user_id)
        .bind(lab_id)
        .bind(&spawn.container_id)
        .bind(&spawn.status)
        .bind(&spawn.webshell_url)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(session)
    }

    /// POST /sessions/stop
    pub async fn stop_session(
        &self,
        session_id: Uuid,
    ) -> Result<(), AppError> {
        // 1️⃣ Get container_id
        let rec = sqlx::query(
            r#"
            SELECT container_id
            FROM lab_sessions
            WHERE session_id = $1
            "#
        )
        .bind(session_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        let container_id: String = rec
            .try_get("container_id")
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // 2️⃣ Call lab-api-service
        self.client
            .post(format!("{}/spawn/stop", self.lab_api_url))
            .json(&serde_json::json!({
                "container_id": container_id
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Stop call failed: {e}")))?;

        // 3️⃣ Update DB
        sqlx::query(
            r#"
            UPDATE lab_sessions
            SET status = 'stopped'
            WHERE session_id = $1
            "#
        )
        .bind(session_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(())
    }
}

#[derive(Deserialize)]
struct SpawnResponse {
    container_id: String,
    webshell_url: String,
    status: String,
}
