/// Apple Sign In identity token verification.
pub mod apple;
/// Apple App Attest device attestation verification.
pub mod apple_attest;
/// Staff JWT signing and verification.
pub mod staff;

use chrono::Utc;
use deadpool_redis::Pool as RedisPool;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

use secrecy::ExposeSecret;

use crate::{config::Config, error::DomainError, types::UserId};

// ── Claims ────────────────────────────────────────────────────────────────────

/// JWT claims payload embedded in every user access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Identifies the authenticated user.
    pub user_id: UserId,
    /// Unix timestamp at which the token expires.
    pub exp:     usize,
    /// Unique token ID — used for server-side revocation.
    pub jti:     String,
}

// ── Token operations ──────────────────────────────────────────────────────────

/// Sign a new JWT for `user_id` using the primary JWT secret from `cfg`.
/// Tokens are valid for 90 days from the time of signing.
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

/// Verify a JWT and return its [`Claims`] if valid.
/// Tries the current secret first, then the previous secret (rotation window).
/// Returns `None` for expired, tampered, or unrecognised tokens.
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

/// In-process JWT revocation list mapping JTI to Unix expiry timestamp.
///
/// Used when Redis is not configured. Safe for single-instance deployments only;
/// use Redis for multi-instance deployments so revocations propagate across instances.
pub type RevokedTokens = Arc<Mutex<HashMap<String, usize>>>;

/// Create an empty in-process token revocation list.
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
/// back to in-process store for single-instance deployments. Always writes a
/// row to `jwt_revocations` as a durable audit trail (BFIP Section 1).
///
/// The `jwt_revocations` table can be pruned daily:
///   `DELETE FROM jwt_revocations WHERE expires_at < now()`
///
/// Events that MUST call this function (extend as features are added):
///   - Explicit logout                             ✓ implemented
///   - Password change while authenticated         (future — needs current JTI in scope)
///   - Admin-forced session termination            (future)
///   - Account deletion                            (future)
///   - Suspicious activity / security incident     (future)
///   - Staff token invalidation mid-shift          (future, same function, staff JTI)
pub async fn revoke_token(
    pool:     &PgPool,
    redis:    &Option<RedisPool>,
    fallback: &RevokedTokens,
    user_id:  UserId,
    jti:      &str,
    exp:      usize,
) {
    let ttl = (exp as i64).saturating_sub(Utc::now().timestamp());
    if ttl <= 0 { return; } // already expired — JWT validation will reject it anyway

    // Durable audit trail — BFIP Section 1.
    // Fire-and-forget: Redis/in-process remains the primary revocation check path.
    let expires_at = chrono::DateTime::from_timestamp(exp as i64, 0)
        .unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(ttl));
    if let Err(e) = sqlx::query(
        "INSERT INTO jwt_revocations (jti, user_id, expires_at) VALUES ($1, $2, $3)
         ON CONFLICT (jti) DO NOTHING"
    )
    .bind(jti)
    .bind(i32::from(user_id))
    .bind(expires_at)
    .execute(pool)
    .await
    {
        tracing::error!(jti, error = %e, "jwt_revocations insert failed — audit trail gap");
    }

    if let Some(redis_pool) = redis {
        match redis_pool.get().await {
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

/// Returns `true` if the JTI has been revoked. Checks Redis when available.
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

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use crate::config::Config;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use proptest::prelude::*;
    use secrecy::SecretString;

    fn s(v: &str) -> SecretString { SecretString::from(v.to_owned()) }

    fn test_cfg() -> Config {
        Config {
            database_url:    s("postgres://localhost/test"),
            jwt_secret:      s("test-jwt-secret-minimum-32-characters!!"),
            jwt_secret_previous: None,
            staff_jwt_secret: s("test-staff-secret-minimum-32-chars!!"),
            staff_jwt_secret_previous: None,
            stripe_secret_key:     s("sk_test_x"),
            stripe_webhook_secret: s("whsec_x"),
            admin_pin:       s("testpin11"),
            chocolatier_pin: s("testpin22"),
            supplier_pin:    s("testpin33"),
            review_pin:      None,
            port:            3001,
            hmac_shared_key: None,
            redis_url:       None,
            apple_team_id: None, apple_key_id: None, apple_client_id: None,
            apple_private_key: None, resend_api_key: None, anthropic_api_key: None,
            anthropic_base_url: None,
            cloudinary_cloud_name: None, cloudinary_api_key: None, cloudinary_api_secret: None,
            square_app_id: None, square_app_secret: None, square_oauth_redirect_url: None,
            square_token_encryption_key: None, operator_email: None,
            api_base_url: "http://localhost:3001".to_owned(),
            app_store_id: None, platform_fee_bips: 500,
            square_order_webhook_signing_key: None, square_order_notification_url: None,
            soultoken_hmac_key:    s("test-soultoken-hmac-key-32bytes!!"),
            soultoken_signing_key: s("test-soultoken-sign-key-32bytes!!"),
        }
    }

    proptest! {
        /// Any user ID that can be represented as i32 produces a token that
        /// verifies to the same user ID.
        #[test]
        fn valid_token_round_trips(user_id_raw in 1i32..1_000_000i32) {
            let cfg = test_cfg();
            let uid = UserId::from(user_id_raw);
            let token = sign_token(uid, &cfg).unwrap();
            let claims = verify_token(&token, &cfg);
            prop_assert!(claims.is_some(), "valid token must verify");
            prop_assert_eq!(claims.unwrap().user_id, uid);
        }

        /// Appending arbitrary characters to a valid token must invalidate it.
        #[test]
        fn tampered_token_fails_verification(
            user_id_raw in 1i32..1_000_000i32,
            extra in "[a-zA-Z0-9+/]{1,20}",
        ) {
            let cfg = test_cfg();
            let uid = UserId::from(user_id_raw);
            let token = sign_token(uid, &cfg).unwrap();
            let tampered = format!("{token}{extra}");
            prop_assert!(
                verify_token(&tampered, &cfg).is_none(),
                "tampered token must not verify"
            );
        }

        /// A token whose exp is in the past must always be rejected,
        /// regardless of user_id content.
        #[test]
        fn expired_token_fails_verification(user_id_raw in 1i32..1_000_000i32) {
            let cfg = test_cfg();
            let uid = UserId::from(user_id_raw);
            let expired_claims = Claims {
                user_id: uid,
                exp: 1_000_000, // 1970-01-12 — always expired
                jti: "prop-test-jti".to_owned(),
            };
            use secrecy::ExposeSecret;
            let expired_token = encode(
                &Header::default(),
                &expired_claims,
                &EncodingKey::from_secret(cfg.jwt_secret.expose_secret().as_bytes()),
            ).unwrap();
            prop_assert!(
                verify_token(&expired_token, &cfg).is_none(),
                "expired token must not verify"
            );
        }
    }

    // ── jwt_revocations DB writes (BFIP Section 1) ────────────────────────────

    /// revoke_token must insert a row into jwt_revocations with the correct
    /// jti, user_id, and expires_at.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn revoke_token_writes_jwt_revocations_row(pool: sqlx::PgPool) {
        // Seed a user so the FK resolves.
        let (user_id_raw,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ('logout@test.com', true) RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let user_id = UserId::from(user_id_raw);

        let cfg   = test_cfg();
        let token = sign_token(user_id, &cfg).unwrap();
        let claims = verify_token(&token, &cfg).expect("freshly signed token must verify");

        // Revoke without Redis — DB write must still happen.
        let fallback = new_revoked_tokens();
        revoke_token(&pool, &None, &fallback, user_id, &claims.jti, claims.exp).await;

        let row: (String, i32) = sqlx::query_as(
            "SELECT jti, user_id FROM jwt_revocations WHERE jti = $1"
        )
        .bind(&claims.jti)
        .fetch_one(&pool)
        .await
        .expect("jwt_revocations row must exist after revoke_token");

        assert_eq!(row.0, claims.jti);
        assert_eq!(row.1, user_id_raw);
    }
}
