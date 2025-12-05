use serde::{Serialize, Deserialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone)]
pub struct Session {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    pub container_id: String,
    pub status: String,
    pub webshell_url: String,
    pub created_at: String,
    pub expires_at: Option<String>,
}
