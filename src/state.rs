use crate::services::sessions_service::SessionsService;

#[derive(Clone)]
pub struct AppState {
    pub sessions_service: SessionsService,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions_service: SessionsService::new(),
        }
    }
}
