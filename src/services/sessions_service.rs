use uuid::Uuid;
use serde::Deserialize;
use reqwest::Client;

use crate::{models::session::Session, error::AppError};

#[derive(Clone)]
pub struct SessionsService {
    client: Client,
    lab_api_url: String,
}

impl SessionsService {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            lab_api_url: std::env::var("LAB_API_URL")
                .unwrap_or_else(|_| "http://localhost:8085".into()),
        }
    }

    pub fn list_sessions(&self, user_id: Uuid) -> Vec<Session> {
        vec![Session {
            session_id: Uuid::new_v4(),
            user_id,
            lab_id: Uuid::new_v4(),
            container_id: "mock-container-123".into(),
            status: "running".into(),
            webshell_url: "ws://localhost:8080/ws/mock".into(),
            created_at: "2025-01-01T12:00:00Z".into(),
            expires_at: None,
        }]
    }

    pub async fn start_session(
        &self,
        user_id: Uuid,
        lab_id: Uuid,
    ) -> Result<Session, AppError> {
        let payload = serde_json::json!({
            "lab_id": lab_id.to_string()
        });

        let resp = self.client
            .post(format!("{}/spawn", self.lab_api_url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Spawn call failed: {}", e)))?;

        let spawn: SpawnResponse = resp
            .json()
            .await
            .map_err(|_| AppError::Internal("Invalid response from LabApiService".into()))?;

        Ok(Session {
            session_id: Uuid::new_v4(),
            user_id,
            lab_id,
            container_id: spawn.container_id,
            status: spawn.status,
            webshell_url: spawn.webshell_url,
            created_at: "2025-01-01T12:00:00Z".into(),
            expires_at: None,
        })
    }

    pub async fn stop_session(&self, session_id: Uuid) -> Result<(), AppError> {
        let payload = serde_json::json!({
            "container_id": session_id.to_string()
        });

        self.client
            .post(format!("{}/spawn/stop", self.lab_api_url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Stop call failed: {}", e)))?;

        Ok(())
    }
}

#[derive(Deserialize)]
pub struct SpawnResponse {
    pub container_id: String,
    pub webshell_url: String,
    pub status: String,
}