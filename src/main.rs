use axum::Router;
use tower_http::cors::{Any, CorsLayer};

mod routes;
mod state;
mod models;
mod error;
mod services;

use crate::routes::init_routes;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    // Create application state
    let state = AppState::new();

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Build Router
    let app = init_routes()
        .with_state(state)
        .layer(cors);

    // Bind server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3003")
        .await
        .unwrap();

    println!("Sessions MS running on http://localhost:3003");

    // Serve
    axum::serve(listener, app)
        .await
        .unwrap();
}
