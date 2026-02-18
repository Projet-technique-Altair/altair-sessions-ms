use tower_http::cors::{Any, CorsLayer};

mod error;
mod middleware;
mod models;
mod routes;
mod services;
mod state;

use crate::routes::init_routes;
use crate::state::AppState;

use crate::middleware::fake_auth::fake_auth;
use axum::middleware::from_fn;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let state = AppState::new().await;

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = init_routes()
        .with_state(state)
        .layer(cors)
        .layer(from_fn(fake_auth));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3003")
        .await
        .expect("Failed to bind port 3003");

    println!("Sessions MS running on http://localhost:3003");

    axum::serve(listener, app).await.unwrap();
}
