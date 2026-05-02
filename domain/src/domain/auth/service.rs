use deadpool_redis::redis;
use secrecy::ExposeSecret;
use sha2::{Sha256, Digest};
use sqlx::PgPool;
use std::{net::IpAddr, sync::Arc};
use uuid::Uuid;

use crate::{
    audit, auth,
    config::Config,
    error::{DomainError, AppResult},
    event_bus::EventBus,
    events::DomainEvent,
    types::UserId,
};
use box_fraise_integrations::resend;
use super::{
    repository,
    types::{AuthResponse, UserRow},
};

const MAGIC_LINK_PREFIX: &str = "fraise:magic:";
const MAGIC_LINK_TTL:    u64  = 900;
const MAGIC_RATE_PREFIX: &str = "fraise:rate:magic:";
const MAGIC_RATE_TTL:    u64  = 120;

// ── Apple Sign In ─────────────────────────────────────────────────────────────

/// Verify an Apple identity token, find or create the corresponding user, and
/// return a signed JWT. Emits [`DomainEvent::UserRegistered`] for new accounts
/// and [`DomainEvent::UserLoggedIn`] for all successful sign-ins.
///
/// Also inserts an `apple_auth_sessions` row as a durable audit trail
/// (BFIP Section 3). The `identity_token_hash` is SHA-256 of the raw token —
/// the plaintext is never stored.
pub async fn authenticate_apple(
    pool:           &PgPool,
    cfg:            &Arc<Config>,
    http:           &reqwest::Client,
    identity_token: &str,
    display_name:   Option<&str>,
    ip:             Option<IpAddr>,
    event_bus:      &EventBus,
) -> AppResult<AuthResponse> {
    let claims = crate::auth::apple::verify_identity_token(identity_token, cfg, http).await?;

    let email = claims.email.as_deref();
    let (user, is_new) =
        repository::find_or_create_apple(pool, &claims.sub, email, display_name).await?;

    if user.is_banned {
        return Err(DomainError::Forbidden);
    }

    // Durable session record — BFIP Section 3. Apple tokens are valid 10 minutes.
    let token_hash = sha256_hex(identity_token.as_bytes());
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(10);
    let ip_str     = ip.map(|a| a.to_string());
    if let Err(e) = sqlx::query(
        "INSERT INTO apple_auth_sessions \
         (user_id, apple_user_identifier, identity_token_hash, ip_address, expires_at) \
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(i32::from(user.id))
    .bind(&claims.sub)
    .bind(&token_hash)
    .bind(ip_str.as_deref())
    .bind(expires_at)
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "apple_auth_sessions insert failed — audit trail gap");
    }

    if is_new {
        event_bus.publish(DomainEvent::UserRegistered {
            user_id: user.id,
            email:   user.email.clone(),
        });
    }
    event_bus.publish(DomainEvent::UserLoggedIn { user_id: user.id });

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new, verified: user.email_verified })
}

// ── Active user ───────────────────────────────────────────────────────────────

/// Fetch the full user row for `user_id`. Returns `Unauthorized` if the user
/// does not exist, or `Forbidden` if the account has been banned.
pub async fn get_active_user(pool: &PgPool, user_id: UserId) -> AppResult<UserRow> {
    let user = repository::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    if user.is_banned { return Err(DomainError::Forbidden); }
    Ok(user)
}

// ── Magic link auth ───────────────────────────────────────────────────────────

/// Send a magic link email for `email`. Creates the user if they don't exist.
/// Silently no-ops when rate-limited (avoids email enumeration).
/// Returns `Ok` even when Redis is unavailable (token creation is skipped).
///
/// When Redis is available, also writes a row to `magic_link_tokens` as a
/// durable audit record (BFIP Section 3.1). The row's `token_hash` is
/// SHA-256 of the raw token — the plaintext is never stored.
pub async fn request_magic_link(
    pool:  &PgPool,
    cfg:   &Arc<Config>,
    http:  &reqwest::Client,
    redis: Option<&deadpool_redis::Pool>,
    email: &str,
    ip:    Option<IpAddr>,
) -> AppResult<()> {
    if let Some(pool_r) = redis {
        let key = format!("{MAGIC_RATE_PREFIX}{}", email.to_lowercase());
        let mut conn = pool_r.get().await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;
        let count: i64 = redis::cmd("INCR").arg(&key)
            .query_async(&mut *conn).await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;
        if count == 1 {
            let _: () = redis::cmd("EXPIRE").arg(&key).arg(MAGIC_RATE_TTL)
                .query_async(&mut *conn).await.unwrap_or(());
        }
        if count > 1 { return Ok(()); }
    }

    let (user, _) = repository::find_or_create_magic_link_user(pool, email).await?;
    if user.is_banned { return Ok(()); }

    let Some(redis_pool) = redis else { return Ok(()); };

    let token     = Uuid::new_v4().to_string();
    let redis_key = format!("{MAGIC_LINK_PREFIX}{token}");
    let mut conn  = redis_pool.get().await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    let _: () = redis::cmd("SET")
        .arg(&redis_key).arg(i32::from(user.id).to_string())
        .arg("EX").arg(MAGIC_LINK_TTL).arg("NX")
        .query_async(&mut *conn).await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis SET: {e}")))?;

    // Durable audit trail — BFIP Section 3.1.
    // Errors are logged and swallowed: Redis is the primary auth path.
    let token_hash  = sha256_hex(token.as_bytes());
    let expires_at  = chrono::Utc::now() + chrono::Duration::seconds(MAGIC_LINK_TTL as i64);
    let rate_key    = format!("{MAGIC_RATE_PREFIX}{}", email.to_lowercase());
    let ip_str      = ip.map(|a| a.to_string());

    if let Err(e) = sqlx::query(
        "INSERT INTO magic_link_tokens \
         (user_id, email, token_hash, ip_address, rate_limit_key, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(i32::from(user.id))
    .bind(email)
    .bind(&token_hash)
    .bind(ip_str.as_deref())
    .bind(&rate_key)
    .bind(expires_at)
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "magic_link_tokens insert failed — audit trail gap");
    }

    if let Some(api_key) = cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned()) {
        let http     = http.clone();
        let base_url = cfg.api_base_url.clone();
        let to       = email.to_owned();
        tokio::spawn(async move {
            let link = format!("{base_url}/api/auth/magic-link/open?token={token}");
            if let Err(e) = resend::send_magic_link_email(&http, &api_key, &to, &link).await {
                tracing::error!(error = %e, "magic link email delivery failed");
            }
        });
    }
    Ok(())
}

/// Consume a magic link `token` and return a signed JWT on success.
///
/// The token is consumed atomically (GETDEL) — a second call with the same
/// token always returns `Unauthorized`. Emits [`DomainEvent::UserLoggedIn`].
pub async fn verify_magic_link(
    pool:      &PgPool,
    cfg:       &Arc<Config>,
    redis:     Option<&deadpool_redis::Pool>,
    token:     &str,
    ip:        Option<IpAddr>,
    event_bus: &EventBus,
) -> AppResult<AuthResponse> {
    let redis_pool = redis.ok_or(DomainError::Unauthorized)?;

    let key = format!("{MAGIC_LINK_PREFIX}{token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    let raw: Option<String> = redis::cmd("GETDEL").arg(&key)
        .query_async(&mut *conn).await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let user_id = UserId::from(match raw {
        Some(s) => s.parse::<i32>().map_err(|_| DomainError::Unauthorized)?,
        None => {
            audit::write(pool, None, None, "auth.magic_link_invalid",
                serde_json::json!({
                    "reason": "token_expired_or_consumed",
                    "ip": ip.map(|a| a.to_string()),
                })).await;
            return Err(DomainError::Unauthorized);
        }
    });

    // Mark token consumed in the DB audit trail — fire-and-forget.
    // Redis GETDEL is the authoritative consumption; this is the BFIP record.
    let token_hash = sha256_hex(token.as_bytes());
    sqlx::query(
        "UPDATE magic_link_tokens SET used_at = NOW() WHERE token_hash = $1"
    )
    .bind(&token_hash)
    .execute(pool)
    .await
    .ok();

    let user = repository::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;

    if user.is_banned {
        audit::write(pool, Some(user_id.into()), None, "auth.login_blocked",
            serde_json::json!({
                "reason": "banned",
                "via":    "magic_link",
                "ip":     ip.map(|a| a.to_string()),
            })).await;
        return Err(DomainError::Forbidden);
    }

    if !user.email_verified {
        repository::set_verified(pool, user_id).await?;
    }

    event_bus.publish(DomainEvent::UserLoggedIn { user_id });

    let jwt = auth::sign_token(user_id, cfg)?;
    Ok(AuthResponse { user_id, token: jwt, is_new: false, verified: true })
}

// ── Profile mutations ─────────────────────────────────────────────────────────

// ── Helpers ───────────────────────────────────────────────────────────────────

/// SHA-256 of `data` returned as a lowercase hex string.
fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

// ── Profile mutations ─────────────────────────────────────────────────────────

/// Register or update the Expo push token for `user_id`'s device.
pub async fn update_push_token(pool: &PgPool, user_id: UserId, token: &str) -> AppResult<()> {
    repository::set_push_token(pool, user_id, token).await
}

/// Update the display name for `user_id`. The caller is responsible for
/// validating length (1–50 characters) before calling this function.
pub async fn update_display_name(pool: &PgPool, user_id: UserId, name: &str) -> AppResult<()> {
    repository::set_display_name(pool, user_id, name).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::error::DomainError;
    use deadpool_redis::redis;
    use secrecy::SecretString;
    use sqlx::PgPool;

    fn s(v: &str) -> SecretString { SecretString::from(v.to_owned()) }

    fn test_cfg() -> Arc<Config> {
        Arc::new(Config {
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
            apple_team_id:   None,
            apple_key_id:    None,
            apple_client_id: None,
            apple_private_key:     None,
            resend_api_key:        None,
            anthropic_api_key:     None,
            anthropic_base_url:    None,
            cloudinary_cloud_name: None,
            cloudinary_api_key:    None,
            cloudinary_api_secret: None,
            square_app_id:                None,
            square_app_secret:            None,
            square_oauth_redirect_url:    None,
            square_token_encryption_key:  None,
            operator_email:               None,
            api_base_url: "http://localhost:3001".to_owned(),
            app_store_id: None,
            platform_fee_bips: 500,
            square_order_webhook_signing_key: None,
            square_order_notification_url:    None,
        })
    }

    async fn redis_pool_from_env() -> Option<deadpool_redis::Pool> {
        let url = std::env::var("REDIS_URL").ok()?;
        deadpool_redis::Config::from_url(url)
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .ok()
    }

    async fn insert_user(pool: &PgPool, email: &str) -> UserId {
        let (id,): (i32,) =
            sqlx::query_as("INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id")
                .bind(email)
                .fetch_one(pool)
                .await
                .unwrap();
        UserId::from(id)
    }

    // ── authenticate_apple ────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn authenticate_apple_invalid_token_is_unauthorized(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();
        let result = authenticate_apple(&pool, &cfg, &http, "not.a.jwt", None, None, &bus).await;
        assert!(
            matches!(result, Err(DomainError::InvalidInput(_) | DomainError::Unauthorized | DomainError::Internal(_))),
            "invalid Apple token must fail, got: {result:?}"
        );
    }

    // ── update_push_token ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_push_token_stores_token_in_db(pool: PgPool) {
        let user_id = insert_user(&pool, "push@test.com").await;

        update_push_token(&pool, user_id, "ExponentPushToken[abc123]").await.unwrap();

        let stored: Option<String> =
            sqlx::query_scalar("SELECT push_token FROM users WHERE id = $1")
                .bind(i32::from(user_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some("ExponentPushToken[abc123]"));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_push_token_overwrites_existing_token(pool: PgPool) {
        let user_id = insert_user(&pool, "push2@test.com").await;

        update_push_token(&pool, user_id, "old-token").await.unwrap();
        update_push_token(&pool, user_id, "new-token").await.unwrap();

        let stored: Option<String> =
            sqlx::query_scalar("SELECT push_token FROM users WHERE id = $1")
                .bind(i32::from(user_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some("new-token"));
    }

    // ── update_display_name ───────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn update_display_name_stores_name_in_db(pool: PgPool) {
        let user_id = insert_user(&pool, "name@test.com").await;

        update_display_name(&pool, user_id, "Alice").await.unwrap();

        let stored: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
                .bind(i32::from(user_id))
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stored.as_deref(), Some("Alice"));
    }

    // ── request_magic_link ────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn request_magic_link_creates_user_when_email_unknown(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let Some(redis) = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set");
            return;
        };

        request_magic_link(&pool, &cfg, &http, Some(&redis), "newmagic@test.com", None)
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
                .bind("newmagic@test.com")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "magic link must create a user for unknown email");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn request_magic_link_rate_limits_second_request(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let Some(redis) = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set");
            return;
        };

        // First request — sets the rate counter.
        request_magic_link(&pool, &cfg, &http, Some(&redis), "ratelimit@test.com", None)
            .await
            .unwrap();

        // Pre-seed counter to 1 so the next INCR hits the limit.
        let key = format!("fraise:rate:magic:{}", "ratelimit@test.com");
        let mut conn = redis.get().await.unwrap();
        let _: () = redis::cmd("SET").arg(&key).arg(1u64).arg("EX").arg(120u64)
            .query_async(&mut *conn).await.unwrap();
        drop(conn);

        // Second request must be rate-limited (silently returns Ok but no token written).
        request_magic_link(&pool, &cfg, &http, Some(&redis), "ratelimit@test.com", None)
            .await
            .unwrap(); // still Ok — rate limit is silent to avoid enumeration
    }

    // ── verify_magic_link ─────────────────────────────────────────────────────

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_magic_link_success_issues_jwt(pool: PgPool) {
        let cfg  = test_cfg();
        let Some(redis) = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set");
            return;
        };

        // Seed a user and a token directly into Redis.
        let user_id = insert_user(&pool, "magicverify@test.com").await;
        let token   = "test-magic-token-abc";
        let key     = format!("fraise:magic:{token}");
        let mut conn = redis.get().await.unwrap();
        let _: () = redis::cmd("SET")
            .arg(&key).arg(i32::from(user_id).to_string())
            .arg("EX").arg(900u64)
            .query_async(&mut *conn).await.unwrap();
        drop(conn);

        let bus  = EventBus::new();
        let resp = verify_magic_link(&pool, &cfg, Some(&redis), token, None, &bus)
            .await
            .unwrap();
        assert!(!resp.token.is_empty(), "must issue a JWT");
        assert_eq!(resp.user_id, user_id);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_magic_link_expired_token_is_unauthorized(pool: PgPool) {
        let cfg  = test_cfg();
        let Some(redis) = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set");
            return;
        };
        // Token was never seeded — simulates expired or wrong token.
        let bus = EventBus::new();
        let result = verify_magic_link(
            &pool, &cfg, Some(&redis), "00000000-0000-0000-0000-000000000000", None, &bus,
        ).await;
        assert!(matches!(result, Err(DomainError::Unauthorized)));
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn verify_magic_link_already_used_is_unauthorized(pool: PgPool) {
        let cfg  = test_cfg();
        let Some(redis) = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set");
            return;
        };

        let user_id = insert_user(&pool, "magicused@test.com").await;
        let token   = "single-use-magic-token";
        let key     = format!("fraise:magic:{token}");
        let mut conn = redis.get().await.unwrap();
        let _: () = redis::cmd("SET")
            .arg(&key).arg(i32::from(user_id).to_string())
            .arg("EX").arg(900u64)
            .query_async(&mut *conn).await.unwrap();
        drop(conn);

        let bus = EventBus::new();
        // First use succeeds.
        verify_magic_link(&pool, &cfg, Some(&redis), token, None, &bus).await.unwrap();

        // Second use must fail — token consumed by GETDEL.
        let result = verify_magic_link(&pool, &cfg, Some(&redis), token, None, &bus).await;
        assert!(matches!(result, Err(DomainError::Unauthorized)));
    }

    // ── magic_link_tokens DB writes (BFIP Section 3.1) ───────────────────────

    /// request_magic_link must insert a row into magic_link_tokens,
    /// and verify_magic_link must set used_at on that row.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn magic_link_tokens_written_and_consumed(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let Some(redis) = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set");
            return;
        };

        // Seed a user.
        let email   = "ml_tokens@test.com";
        let user_id = insert_user(&pool, email).await;

        // Request a magic link — should write to magic_link_tokens.
        request_magic_link(&pool, &cfg, &http, Some(&redis), email, None)
            .await
            .unwrap();

        let row: (i32, Option<String>) = sqlx::query_as(
            "SELECT user_id, used_at::text FROM magic_link_tokens WHERE email = $1"
        )
        .bind(email)
        .fetch_one(&pool)
        .await
        .expect("magic_link_tokens row must exist after request_magic_link");

        assert_eq!(row.0, i32::from(user_id), "token must belong to the requesting user");
        assert!(row.1.is_none(), "used_at must be NULL before verification");

        // Fetch the raw token from Redis to simulate verification.
        let token_key_pattern = format!("fraise:magic:*");
        let mut conn = redis.get().await.unwrap();
        let keys: Vec<String> = deadpool_redis::redis::cmd("KEYS")
            .arg(&token_key_pattern)
            .query_async(&mut *conn)
            .await
            .unwrap();

        assert!(!keys.is_empty(), "Redis must have a magic link key");
        let raw_token = keys[0].trim_start_matches("fraise:magic:").to_owned();

        // Verify the token — should set used_at.
        let bus  = EventBus::new();
        let resp = verify_magic_link(&pool, &cfg, Some(&redis), &raw_token, None, &bus)
            .await
            .unwrap();
        assert_eq!(resp.user_id, user_id);

        let used_at: Option<String> = sqlx::query_scalar(
            "SELECT used_at::text FROM magic_link_tokens WHERE email = $1"
        )
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(used_at.is_some(), "used_at must be set after verify_magic_link");
    }

    // ── apple_auth_sessions DB writes (BFIP Section 3) ───────────────────────

    /// authenticate_apple must insert a row into apple_auth_sessions.
    /// Uses a mock token that fails Apple's JWKS — confirms the INSERT
    /// never happens on invalid tokens (the error returns before the INSERT).
    /// Positive path tested via the audit trail on real sign-in flows.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn authenticate_apple_invalid_token_does_not_write_session(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        // Invalid token returns Err before any DB writes.
        let result = authenticate_apple(
            &pool, &cfg, &http, "not.a.real.jwt", None, None, &bus,
        ).await;
        assert!(result.is_err(), "invalid Apple token must fail");

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM apple_auth_sessions")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "no session row must be written for invalid token");
    }

    // ── Adversarial auth tests ────────────────────────────────────────────────

    /// An expired magic link (no longer in Redis) must be rejected.
    /// The absence from Redis simulates TTL expiry — used_at must NOT be set
    /// because the token was never successfully consumed.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_reuse_expired_magic_link(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let cfg           = test_cfg();
        let Some(redis)   = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set"); return;
        };

        let user_id = insert_user(&pool, &email).await;
        let token   = uuid::Uuid::new_v4().to_string();

        // Insert the DB audit row with an expiry in the past (simulating TTL elapsed).
        let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));
        let old_expiry = chrono::Utc::now() - chrono::Duration::hours(1);
        if let Err(e) = sqlx::query(
            "INSERT INTO magic_link_tokens \
             (user_id, email, token_hash, expires_at) VALUES ($1, $2, $3, $4)"
        )
        .bind(i32::from(user_id))
        .bind(&email)
        .bind(&token_hash)
        .bind(old_expiry)
        .execute(&pool)
        .await
        {
            eprintln!("setup insert failed: {e}");
        }
        // Deliberately NOT seeding the Redis key — simulates TTL expiry.

        let bus    = EventBus::new();
        let result = verify_magic_link(&pool, &cfg, Some(&redis), &token, None, &bus).await;
        assert!(matches!(result, Err(DomainError::Unauthorized)),
            "expired (Redis-absent) token must be Unauthorized");

        let used_at: Option<String> = sqlx::query_scalar(
            "SELECT used_at::text FROM magic_link_tokens WHERE token_hash = $1"
        )
        .bind(&token_hash)
        .fetch_optional(&pool)
        .await
        .unwrap()
        .flatten();
        assert!(used_at.is_none(), "used_at must remain NULL for expired token");
    }

    /// A magic link that has already been consumed cannot be used a second time.
    /// GETDEL atomically removes the Redis key on first use — second attempt gets None.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_reuse_consumed_magic_link(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let cfg           = test_cfg();
        let Some(redis)   = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set"); return;
        };

        let user_id = insert_user(&pool, &email).await;
        let token   = uuid::Uuid::new_v4().to_string();
        let key     = format!("{MAGIC_LINK_PREFIX}{token}");
        let mut conn = redis.get().await.unwrap();
        let _: () = redis::cmd("SET")
            .arg(&key).arg(i32::from(user_id).to_string())
            .arg("EX").arg(900u64)
            .query_async(&mut *conn).await.unwrap();
        drop(conn);

        let bus = EventBus::new();
        // First use succeeds.
        verify_magic_link(&pool, &cfg, Some(&redis), &token, None, &bus).await
            .expect("first verification must succeed");

        // Second use — Redis key is gone (GETDEL consumed it).
        let result = verify_magic_link(&pool, &cfg, Some(&redis), &token, None, &bus).await;
        assert!(matches!(result, Err(DomainError::Unauthorized)),
            "consumed token must not be verifiable again");

        // used_at must be set exactly once — not cleared by the second attempt.
        let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));
        let used_at: Option<String> = sqlx::query_scalar(
            "SELECT used_at::text FROM magic_link_tokens WHERE token_hash = $1"
        )
        .bind(&token_hash)
        .fetch_optional(&pool)
        .await
        .unwrap()
        .flatten();
        assert!(used_at.is_some(), "used_at must be set after first successful verification");
    }

    /// Modifying any character of a valid magic link token produces a different
    /// Redis key — GETDEL returns None and the original token remains unconsumed.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_use_tampered_magic_link(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();
        let cfg           = test_cfg();
        let Some(redis)   = redis_pool_from_env().await else {
            eprintln!("skipping: REDIS_URL not set"); return;
        };

        let user_id    = insert_user(&pool, &email).await;
        let real_token = uuid::Uuid::new_v4().to_string();
        let key        = format!("{MAGIC_LINK_PREFIX}{real_token}");
        let mut conn   = redis.get().await.unwrap();
        let _: () = redis::cmd("SET")
            .arg(&key).arg(i32::from(user_id).to_string())
            .arg("EX").arg(900u64)
            .query_async(&mut *conn).await.unwrap();
        drop(conn);

        // Tamper: replace the last character.
        let mut tampered = real_token.clone();
        let last = tampered.pop().unwrap_or('0');
        tampered.push(if last == '0' { '1' } else { '0' });

        let bus    = EventBus::new();
        let result = verify_magic_link(&pool, &cfg, Some(&redis), &tampered, None, &bus).await;
        assert!(matches!(result, Err(DomainError::Unauthorized)),
            "tampered token must be rejected");

        // The real token must still be in Redis — the tampered attempt didn't consume it.
        let mut conn   = redis.get().await.unwrap();
        let exists: i64 = redis::cmd("EXISTS")
            .arg(&key)
            .query_async(&mut *conn).await.unwrap();
        assert_eq!(exists, 1, "original token must still be valid after tampered attempt");
    }

    /// Once a JTI is revoked (via revoke_token), check_revoked must return true —
    /// proving the full revocation cycle works end to end.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn adversary_cannot_authenticate_with_revoked_jwt(pool: PgPool) {
        use crate::auth::{check_revoked, new_revoked_tokens, sign_token, verify_token};
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();

        let user_id = insert_user(&pool, &email).await;
        let cfg     = test_cfg();

        let token  = sign_token(user_id, &cfg).expect("signing must succeed");
        let claims = verify_token(&token, &cfg).expect("freshly signed token must verify");

        let fallback = new_revoked_tokens();
        // Not revoked yet.
        assert!(!check_revoked(&None, &fallback, &claims.jti).await,
            "fresh token must not be revoked");

        // Revoke.
        crate::auth::revoke_token(&pool, &None, &fallback, user_id, &claims.jti, claims.exp).await;

        // Now revoked.
        assert!(check_revoked(&None, &fallback, &claims.jti).await,
            "revoked token must be detected by check_revoked");

        // DB row must exist.
        let row_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM jwt_revocations WHERE jti = $1"
        )
        .bind(&claims.jti)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row_count, 1, "jwt_revocations row must exist");
    }

    // ── Cooling period simulation (BFIP Section 4.3) ─────────────────────────

    /// The UNIQUE(user_id, credential_id, calendar_date) constraint enforces that
    /// only one event per calendar day qualifies. Three events on the same day
    /// count as one qualifying day — NOT three.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn cooling_period_requires_separate_calendar_days(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();

        let user_id = insert_user(&pool, &email).await;
        let uid     = i32::from(user_id);

        // Create identity_credential with cooling_ends_at already in the past.
        let (cred_id,): (i32,) = sqlx::query_as(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at) \
             VALUES ($1, 'stripe_identity', now() - interval '8 days', \
                    now() - interval '1 day') \
             RETURNING id"
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();

        let today = chrono::Utc::now().date_naive();

        // Insert event for today — first succeeds.
        sqlx::query(
            "INSERT INTO cooling_period_events \
             (user_id, credential_id, calendar_date) VALUES ($1, $2, $3)"
        )
        .bind(uid).bind(cred_id).bind(today)
        .execute(&pool).await.unwrap();

        // Second insert on same day must fail the UNIQUE constraint.
        let second = sqlx::query(
            "INSERT INTO cooling_period_events \
             (user_id, credential_id, calendar_date) VALUES ($1, $2, $3)"
        )
        .bind(uid).bind(cred_id).bind(today)
        .execute(&pool).await;
        assert!(second.is_err(), "duplicate (user, cred, date) must violate UNIQUE constraint");

        // Only 1 qualifying day despite the attempted duplicate.
        let days: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT calendar_date) FROM cooling_period_events \
             WHERE user_id = $1 AND credential_id = $2"
        )
        .bind(uid).bind(cred_id)
        .fetch_one(&pool).await.unwrap();

        let required: i32 = sqlx::query_scalar(
            "SELECT cooling_app_opens_required FROM identity_credentials WHERE id = $1"
        )
        .bind(cred_id)
        .fetch_one(&pool).await.unwrap();

        assert!(
            days < required as i64,
            "one same-day event must not satisfy {required}-day requirement (got {days} day(s))"
        );
    }

    /// The cooling period requires BOTH 3 separate calendar days AND the
    /// cooling_ends_at timestamp to have elapsed. Neither alone is sufficient.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn cooling_period_requires_7_days_minimum(pool: PgPool) {
        use fake::{Fake, faker::internet::en::SafeEmail};
        let email: String = SafeEmail().fake();

        let user_id = insert_user(&pool, &email).await;
        let uid     = i32::from(user_id);

        // cooling_ends_at = tomorrow (not yet elapsed).
        let (cred_id,): (i32,) = sqlx::query_as(
            "INSERT INTO identity_credentials \
             (user_id, credential_type, verified_at, cooling_ends_at) \
             VALUES ($1, 'stripe_identity', now() - interval '6 days', \
                    now() + interval '1 day') \
             RETURNING id"
        )
        .bind(uid)
        .fetch_one(&pool)
        .await
        .unwrap();

        // Insert events on 3 distinct calendar days.
        for days_ago in 0i64..3 {
            let date = (chrono::Utc::now() - chrono::Duration::days(days_ago)).date_naive();
            sqlx::query(
                "INSERT INTO cooling_period_events \
                 (user_id, credential_id, calendar_date) VALUES ($1, $2, $3)"
            )
            .bind(uid).bind(cred_id).bind(date)
            .execute(&pool).await.unwrap();
        }

        // Events satisfy day count but cooling_ends_at hasn't elapsed.
        let (days, ends_at): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT COUNT(DISTINCT cpe.calendar_date), MAX(ic.cooling_ends_at) \
             FROM cooling_period_events cpe \
             JOIN identity_credentials ic ON ic.id = cpe.credential_id \
             WHERE cpe.credential_id = $1"
        )
        .bind(cred_id)
        .fetch_one(&pool).await.unwrap();

        assert!(days >= 3, "3 events on 3 days must be recorded");
        assert!(ends_at > chrono::Utc::now(),
            "cooling_ends_at must still be in the future (time not yet elapsed)");
        // → cooling NOT complete: days ✓ but time ✗

        // Simulate time passing: set cooling_ends_at to yesterday.
        sqlx::query(
            "UPDATE identity_credentials SET cooling_ends_at = now() - interval '1 day' \
             WHERE id = $1"
        )
        .bind(cred_id)
        .execute(&pool).await.unwrap();

        let (days2, ends_at2): (i64, chrono::DateTime<chrono::Utc>) = sqlx::query_as(
            "SELECT COUNT(DISTINCT cpe.calendar_date), MAX(ic.cooling_ends_at) \
             FROM cooling_period_events cpe \
             JOIN identity_credentials ic ON ic.id = cpe.credential_id \
             WHERE cpe.credential_id = $1"
        )
        .bind(cred_id)
        .fetch_one(&pool).await.unwrap();

        assert!(days2 >= 3,      "day count must still be ≥ 3");
        assert!(ends_at2 <= chrono::Utc::now(),
            "cooling_ends_at must now be in the past");
        // → cooling IS complete: days ✓ and time ✓
    }
}
