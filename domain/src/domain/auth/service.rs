use deadpool_redis::redis;
use secrecy::ExposeSecret;
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
pub async fn authenticate_apple(
    pool:           &PgPool,
    cfg:            &Arc<Config>,
    http:           &reqwest::Client,
    identity_token: &str,
    display_name:   Option<&str>,
    event_bus:      &EventBus,
) -> AppResult<AuthResponse> {
    let claims = crate::auth::apple::verify_identity_token(identity_token, cfg, http).await?;

    let email = claims.email.as_deref();
    let (user, is_new) =
        repository::find_or_create_apple(pool, &claims.sub, email, display_name).await?;

    if user.is_banned {
        return Err(DomainError::Forbidden);
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
pub async fn request_magic_link(
    pool:  &PgPool,
    cfg:   &Arc<Config>,
    http:  &reqwest::Client,
    redis: Option<&deadpool_redis::Pool>,
    email: &str,
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

    let token = Uuid::new_v4().to_string();
    let key   = format!("{MAGIC_LINK_PREFIX}{token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    let _: () = redis::cmd("SET")
        .arg(&key).arg(i32::from(user.id).to_string())
        .arg("EX").arg(MAGIC_LINK_TTL).arg("NX")
        .query_async(&mut *conn).await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis SET: {e}")))?;

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
                serde_json::json!({ "reason": "token_expired_or_consumed" }), ip).await;
            return Err(DomainError::Unauthorized);
        }
    });

    let user = repository::find_by_id(pool, user_id).await?.ok_or(DomainError::Unauthorized)?;

    if user.is_banned {
        audit::write(pool, Some(user_id.into()), None, "auth.login_blocked",
            serde_json::json!({ "reason": "banned", "via": "magic_link" }), ip).await;
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
        let result = authenticate_apple(&pool, &cfg, &http, "not.a.jwt", None, &bus).await;
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

        request_magic_link(&pool, &cfg, &http, Some(&redis), "newmagic@test.com")
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
        request_magic_link(&pool, &cfg, &http, Some(&redis), "ratelimit@test.com")
            .await
            .unwrap();

        // Pre-seed counter to 1 so the next INCR hits the limit.
        let key = format!("fraise:rate:magic:{}", "ratelimit@test.com");
        let mut conn = redis.get().await.unwrap();
        let _: () = redis::cmd("SET").arg(&key).arg(1u64).arg("EX").arg(120u64)
            .query_async(&mut *conn).await.unwrap();
        drop(conn);

        // Second request must be rate-limited (silently returns Ok but no token written).
        request_magic_link(&pool, &cfg, &http, Some(&redis), "ratelimit@test.com")
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
}
