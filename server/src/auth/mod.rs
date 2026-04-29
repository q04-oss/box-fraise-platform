pub mod apple;
pub mod device;

use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use crate::{config::Config, error::AppError};

// ── Claims ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: i32,
    pub exp:     usize,
    /// Unique token ID — used for server-side revocation.
    pub jti:     String,
}

// ── Token operations ─────────────────────────────────────────────────────────

pub fn sign_token(user_id: i32, cfg: &Config) -> Result<String, AppError> {
    let exp = Utc::now()
        .checked_add_signed(chrono::Duration::days(90))
        .unwrap()
        .timestamp() as usize;

    let claims = Claims {
        user_id,
        exp,
        jti: Uuid::new_v4().to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("token encoding failed: {e}")))
}

pub fn verify_token(token: &str, cfg: &Config) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(cfg.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

// ── Revocation list ──────────────────────────────────────────────────────────

/// In-process JWT revocation set keyed by JTI.
/// Move to Redis before running multiple server instances.
pub type RevokedTokens = Arc<Mutex<HashSet<String>>>;

pub fn new_revoked_tokens() -> RevokedTokens {
    Arc::new(Mutex::new(HashSet::new()))
}

pub fn revoke(list: &RevokedTokens, jti: &str) {
    list.lock().unwrap().insert(jti.to_owned());
}

pub fn is_revoked(list: &RevokedTokens, jti: &str) -> bool {
    list.lock().unwrap().contains(jti)
}
