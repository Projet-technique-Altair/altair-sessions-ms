use uuid::Uuid;
use serde::Deserialize;
use crate::error::AppError;

#[derive(Clone, Debug, Deserialize)]
pub struct JwtClaims {
    pub user_id: Uuid,
    pub role: String, // learner | creator | admin
}


use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};


pub struct AuthUser(pub JwtClaims);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<JwtClaims>()
            .cloned()
            .map(AuthUser)
            .ok_or(AppError::Unauthorized("Missing JWT claims".into()))
    }
}
