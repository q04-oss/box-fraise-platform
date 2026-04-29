use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::RequestPartsExt;
use axum_extra::{
    headers::{authorization::Bearer, Authorization},
    TypedHeader,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env,
    sync::{Arc, Mutex},
};
use uuid::Uuid;
use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: i32,
    pub exp:     usize,
    pub jti:     String,
}

pub type RevokedTokens = Arc<Mutex<HashSet<String>>>;

pub fn new_revoked_tokens() -> RevokedTokens {
    Arc::new(Mutex::new(HashSet::new()))
}

fn secret() -> Result<String, AppError> {
    env::var("JWT_SECRET").map_err(|_| AppError::Internal(anyhow::anyhow!("JWT_SECRET not set")))
}

pub fn sign_token(user_id: i32) -> Result<String, AppError> {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(90))
        .unwrap()
        .timestamp() as usize;
    let claims = Claims { user_id, exp, jti: Uuid::new_v4().to_string() };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret()?.as_bytes()))
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
}

pub fn verify_token(token: &str) -> Option<Claims> {
    let key = secret().ok()?;
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(key.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

// Extracts user_id; use RequireClaims when you also need jti (e.g. logout).
pub struct RequireUser(pub i32);

impl<S> FromRequestParts<S> for RequireUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let RequireClaims(claims) = RequireClaims::from_request_parts(parts, _state).await?;
        Ok(RequireUser(claims.user_id))
    }
}

// Extracts full Claims — use this when you need jti for revocation.
pub struct RequireClaims(pub Claims);

impl<S> FromRequestParts<S> for RequireClaims
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

        if let Some(revoked) = parts.extensions.get::<RevokedTokens>() {
            if revoked.lock().unwrap().contains(&claims.jti) {
                return Err(AppError::Unauthorized);
            }
        }

        Ok(RequireClaims(claims))
    }
}
