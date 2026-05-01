pub mod apple;
pub mod apple_attest;
pub mod device;
pub mod staff;

use chrono::Utc;
use deadpool_redis::Pool as RedisPool;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use secrecy::ExposeSecret;

use crate::{config::Config, error::DomainError, types::UserId};

// ── Claims ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub user_id: UserId,
    pub exp:     usize,
    /// Unique token ID — used for server-side revocation.
    pub jti:     String,
}

// ── Token operations ──────────────────────────────────────────────────────────

pub fn sign_token(user_id: UserId, cfg: &Config) -> Result<String, DomainError> {
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
    .map_err(|e| DomainError::Internal(anyhow::anyhow!("token encoding failed: {e}")))
}

pub fn verify_token(token: &str, cfg: &Config) -> Option<Claims> {
    // Try current secret first. Fall back to previous during rotation window.
    decode_claims(token, cfg.jwt_secret.expose_secret())
        .or_else(|| {
            cfg.jwt_secret_previous
                .as_ref()
                .and_then(|prev| decode_claims(token, prev.expose_secret()))
        })
}

fn decode_claims(token: &str, secret: &str) -> Option<Claims> {
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &Validation::new(Algorithm::HS256))
        .ok()
        .map(|d| d.claims)
}

// ── Revocation list ───────────────────────────────────────────────────────────

const REVOKED_KEY_PREFIX: &str = "fraise:revoked:";

/// JTI → Unix expiry. In-process store used when Redis is not configured.
/// Safe for single-instance deployments; replaced by Redis for multi-instance.
pub type RevokedTokens = Arc<Mutex<HashMap<String, usize>>>;

pub fn new_revoked_tokens() -> RevokedTokens {
    Arc::new(Mutex::new(HashMap::new()))
}

// Sync primitives kept for the in-process fallback path.
fn revoke_local(list: &RevokedTokens, jti: &str, exp: usize) {
    list.lock().unwrap().insert(jti.to_owned(), exp);
}

fn is_revoked_local(list: &RevokedTokens, jti: &str) -> bool {
    let now = Utc::now().timestamp() as usize;
    let mut map = list.lock().unwrap();
    map.retain(|_, &mut exp| exp > now);
    map.contains_key(jti)
}

/// Revoke a token by JTI. Uses Redis when available (cross-instance), falls
/// back to in-process store for single-instance deployments.
///
/// Events that MUST call this function (extend as features are added):
///   - Explicit logout                             ✓ implemented
///   - Password change while authenticated         (future — needs current JTI in scope)
///   - Admin-forced session termination            (future)
///   - Account deletion                            (future)
///   - Suspicious activity / security incident     (future)
///   - Staff token invalidation mid-shift          (future, same function, staff JTI)
pub async fn revoke_token(redis: &Option<RedisPool>, fallback: &RevokedTokens, jti: &str, exp: usize) {
    let ttl = (exp as i64).saturating_sub(Utc::now().timestamp());
    if ttl <= 0 { return; } // already expired — JWT validation will reject it anyway

    if let Some(pool) = redis {
        match pool.get().await {
            Ok(mut conn) => {
                let key = format!("{REVOKED_KEY_PREFIX}{jti}");
                if let Err(e) = deadpool_redis::redis::cmd("SET")
                    .arg(&key).arg("1").arg("EX").arg(ttl)
                    .query_async::<_, ()>(&mut *conn)
                    .await
                {
                    tracing::error!(jti, error = %e, "Redis revocation failed — using in-process fallback");
                    revoke_local(fallback, jti, exp);
                }
            }
            Err(e) => {
                tracing::error!(jti, error = %e, "Redis pool error during revocation — using in-process fallback");
                revoke_local(fallback, jti, exp);
            }
        }
    } else {
        revoke_local(fallback, jti, exp);
    }
}

/// Returns true if the JTI has been revoked. Checks Redis when available.
/// On Redis failure, falls back to in-process list rather than failing open.
pub async fn check_revoked(redis: &Option<RedisPool>, fallback: &RevokedTokens, jti: &str) -> bool {
    if let Some(pool) = redis {
        match pool.get().await {
            Ok(mut conn) => {
                match deadpool_redis::redis::cmd("EXISTS")
                    .arg(format!("{REVOKED_KEY_PREFIX}{jti}"))
                    .query_async::<_, i64>(&mut *conn)
                    .await
                {
                    Ok(n)  => n > 0,
                    Err(e) => {
                        tracing::warn!(jti, error = %e, "Redis revocation check failed — using in-process fallback");
                        is_revoked_local(fallback, jti)
                    }
                }
            }
            Err(e) => {
                tracing::warn!(jti, error = %e, "Redis pool error during revocation check — using in-process fallback");
                is_revoked_local(fallback, jti)
            }
        }
    } else {
        is_revoked_local(fallback, jti)
    }
}
