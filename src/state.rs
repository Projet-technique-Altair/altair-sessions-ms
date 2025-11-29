use std::sync::{Arc, Mutex};

use crate::models::session::LabSession;

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<Mutex<Vec<LabSession>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(vec![])),
        }
    }
}
