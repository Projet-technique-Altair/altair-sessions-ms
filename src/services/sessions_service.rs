use uuid::Uuid;

use crate::{models::session::Session, error::AppError};

#[derive(Clone)]
pub struct SessionsService {}

impl SessionsService {
    pub fn new() -> Self {
        Self {}
    }

    /// MVP mock sessions
    pub fn list_sessions(&self, user_id: Uuid) -> Vec<Session> {
        vec![
            Session {
                session_id: Uuid::new_v4(),
                user_id,
                lab_id: Uuid::new_v4(),
                container_id: "mock-container-123".to_string(),
                status: "running".to_string(),
                webshell_url: "ws://localhost:8080/ws/mock".to_string(),
                created_at: "2025-01-01T12:00:00Z".to_string(),
                expires_at: None,
            }
        ]
    }

    /// Create a mock session
    pub fn start_session(&self, user_id: Uuid, lab_id: Uuid) -> Session {
        Session {
            session_id: Uuid::new_v4(),
            user_id,
            lab_id,
            container_id: "mock-container".to_string(),
            status: "running".to_string(),
            webshell_url: "ws://localhost:8080/ws/mock".to_string(),
            created_at: "2025-01-01T12:00:00Z".to_string(),
            expires_at: None,
        }
    }

    /// Stop a mock session
    pub fn stop_session(&self, _session_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
}
