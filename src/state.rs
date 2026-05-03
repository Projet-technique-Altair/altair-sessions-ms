/**
 * @file state — shared application state.
 *
 * @remarks
 * Defines the shared state injected into Axum routes for the Sessions
 * microservice. This state centralizes long-lived service dependencies
 * that must be reused across request handlers.
 *
 * Responsibilities:
 *
 *  - Read the database connection string from environment variables
 *  - Initialize the PostgreSQL connection pool
 *  - Create the Sessions service layer
 *  - Expose shared dependencies through `AppState`
 *  - Allow application state to be cloned for Axum handlers
 *
 * Key characteristics:
 *
 *  - Uses `DATABASE_URL` as required configuration
 *  - Establishes a PostgreSQL connection through `sqlx::PgPool`
 *  - Stores the session service as the main business dependency
 *  - Designed for dependency injection into route handlers
 *
 * This module acts as the composition point between infrastructure
 * resources and the Sessions microservice business layer.
 *
 * @packageDocumentation
 */

use crate::services::sessions_service::SessionsService;
use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub sessions_service: SessionsService,
}

impl AppState {
    pub async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        let db = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database");

        Self {
            sessions_service: SessionsService::new(db),
        }
    }
}
