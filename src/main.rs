use axum::{Router, routing::{get, post}};
use tower_http::cors::{CorsLayer, Any};

mod routes;
mod models;

#[tokio::main]
async fn main() {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .merge(routes::sessions::sessions_routes())
        .merge(routes::labs::labs_routes())
        .layer(cors);

    let addr = "0.0.0.0:3002";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    println!("Sessions-MS running at http://{}", addr);

    axum::serve(listener, app)
        .await
        .unwrap();
}
