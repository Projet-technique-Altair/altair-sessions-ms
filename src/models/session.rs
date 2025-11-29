use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct LabSession {
    pub session_id: String,
    pub user_id: String,
    pub lab_id: String,
    pub container_id: String,
    pub status: String,
    pub webshell_url: String,
}
