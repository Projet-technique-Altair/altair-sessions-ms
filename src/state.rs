use sqlx::PgPool;
use crate::services::sessions_service::SessionsService;

#[derive(Clone)]
pub struct AppState {
    pub sessions_service: SessionsService,
}

impl AppState {
    pub async fn new() -> Self {
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        let db = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database");

        Self {
            sessions_service: SessionsService::new(db),
        }
    }
}
