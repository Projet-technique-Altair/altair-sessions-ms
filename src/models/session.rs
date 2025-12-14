use serde::{Serialize, Deserialize};
use uuid::Uuid;
use sqlx::FromRow;

#[derive(Serialize, Deserialize, Clone, FromRow)]
pub struct Session {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub lab_id: Uuid,
    pub container_id: String,
    pub status: String,
    pub webshell_url: String,

    // IMPORTANT : utiliser chrono, PAS String
    pub created_at: chrono::NaiveDateTime,
    pub expires_at: Option<chrono::NaiveDateTime>,
}
