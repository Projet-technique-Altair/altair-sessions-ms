/**
 * @file main — application entry point.
 *
 * @remarks
 * Bootstraps the Sessions microservice by loading environment
 * configuration, initializing shared application state, configuring
 * middleware, registering HTTP routes, and starting the Axum server.
 *
 * Responsibilities:
 *
 *  - Load environment variables from `.env` when available
 *  - Initialize the shared application state
 *  - Configure CORS middleware from allowed origins
 *  - Register application routes
 *  - Attach shared state to the router
 *  - Start the HTTP server on the configured port
 *
 * Key characteristics:
 *
 *  - Uses environment-driven configuration
 *  - Supports configurable allowed frontend origins
 *  - Provides default local development origins
 *  - Exposes the service through an Axum HTTP server
 *  - Uses a default port when no `PORT` variable is provided
 *
 * This module wires together the core Sessions MS components
 * and starts the HTTP API used to manage session-related features.
 *
 * @packageDocumentation
 */

use axum::http::HeaderValue;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

mod error;
mod models;
mod routes;
mod services;
mod state;

use crate::routes::init_routes;
use crate::state::AppState;

const DEFAULT_ALLOWED_ORIGINS: &str = "http://localhost:5173,http://localhost:3000";
const DEFAULT_PORT: &str = "3003";

fn parse_allowed_origins() -> Vec<HeaderValue> {
    std::env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| DEFAULT_ALLOWED_ORIGINS.to_string())
        .split(',')
        .filter_map(|origin| HeaderValue::from_str(origin.trim()).ok())
        .collect()
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let state = AppState::new().await;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(parse_allowed_origins()))
        .allow_methods(Any)
        .allow_headers(Any);

    let app = init_routes().with_state(state).layer(cors);

    let port = std::env::var("PORT").unwrap_or_else(|_| DEFAULT_PORT.to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap_or_else(|_| panic!("Failed to bind sessions-ms port {port}"));

    println!("Sessions MS running on http://localhost:{port}");

    axum::serve(listener, app).await.unwrap();
}
