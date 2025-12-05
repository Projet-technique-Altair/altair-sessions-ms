use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::{json, Value};

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Session not found")]
    NotFound,

    #[error("Internal error")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body: Json<Value> = Json(json!({
            "error": self.to_string()
        }));

        (status, body).into_response()
    }
}
