use axum::{body::Body, http::Request, middleware::Next, response::Response};
use uuid::Uuid;

pub async fn fake_auth(mut req: Request<Body>, next: Next) -> Response {
    let fake_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    req.extensions_mut().insert(fake_user_id);

    next.run(req).await
}
