use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

mod error;
mod models;
mod routes;
mod services;
mod state;

use routes::{
    health::health,
    metrics::basic_metrics,
    sessions::{get_sessions, start_session, stop_session},
};

#[tokio::main]
async fn main() {
    let state = state::AppState::new();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics/basic", get(basic_metrics))
        .route("/sessions", get(get_sessions))
        .route("/sessions/start", post(start_session))
        .route("/sessions/stop", post(stop_session))
        .with_state(state)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3003").await.unwrap();
    println!("Sessions MS running on http://localhost:3003");

    axum::serve(listener, app).await.unwrap();
}
