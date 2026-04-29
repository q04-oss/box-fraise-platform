use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::RequestPartsExt;
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::env;
use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: i32,
    pub exp:     usize,
}

fn secret() -> String {
    env::var("JWT_SECRET").expect("JWT_SECRET must be set")
}

pub fn sign_token(user_id: i32) -> String {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(90))
        .unwrap()
        .timestamp() as usize;
    let claims = Claims { user_id, exp };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret().as_bytes()))
        .expect("token encoding failed")
}

pub fn verify_token(token: &str) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret().as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

// Axum extractor — use `RequireUser` as a parameter to enforce auth on a route.
// Routes that don't need auth simply omit the parameter.
pub struct RequireUser(pub i32);

impl<S> FromRequestParts<S> for RequireUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AppError::Unauthorized)?;

        let claims = verify_token(bearer.token()).ok_or(AppError::Unauthorized)?;
        Ok(RequireUser(claims.user_id))
    }
}
