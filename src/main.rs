use axum::Router;
use tokio::net::TcpListener;

mod models;
mod routes;
mod state;

use routes::{sessions_routes, health_routes, metrics_routes};
use state::AppState;

#[tokio::main]
async fn main() {
    let state = AppState::new();

    let app = Router::new()
        .nest("/", sessions_routes())
        .nest("/health", health_routes())
        .nest("/metrics/basic", metrics_routes())
        .with_state(state);

    let addr = "0.0.0.0:3003";
    println!("Sessions-MS running on {}", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
