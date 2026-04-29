pub mod apple;
pub mod apple_attest;
pub mod device;

use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use secrecy::ExposeSecret;

use crate::{config::Config, error::AppError};

// ── Claims ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: i32,
    pub exp:     usize,
    /// Unique token ID — used for server-side revocation.
    pub jti:     String,
}

// ── Token operations ──────────────────────────────────────────────────────────

pub fn sign_token(user_id: i32, cfg: &Config) -> Result<String, AppError> {
    let exp = Utc::now()
        .checked_add_signed(chrono::Duration::days(90))
        .unwrap()
        .timestamp() as usize;

    let claims = Claims { user_id, exp, jti: Uuid::new_v4().to_string() };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.jwt_secret.expose_secret().as_bytes()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("token encoding failed: {e}")))
}

pub fn verify_token(token: &str, cfg: &Config) -> Option<Claims> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(cfg.jwt_secret.expose_secret().as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}

// ── Revocation list ───────────────────────────────────────────────────────────

/// JTI → Unix expiry. Entries are self-pruning: once a token's `exp` has
/// passed it can no longer be valid regardless, so it is removed on the next
/// access rather than stored forever.
pub type RevokedTokens = Arc<Mutex<HashMap<String, usize>>>;

pub fn new_revoked_tokens() -> RevokedTokens {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn revoke(list: &RevokedTokens, jti: &str, exp: usize) {
    list.lock().unwrap().insert(jti.to_owned(), exp);
}

pub fn is_revoked(list: &RevokedTokens, jti: &str) -> bool {
    let now = Utc::now().timestamp() as usize;
    let mut map = list.lock().unwrap();
    // Prune entries whose tokens have already expired — they cannot be replayed.
    map.retain(|_, &mut exp| exp > now);
    map.contains_key(jti)
}
