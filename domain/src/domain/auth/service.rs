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

const VERIFY_PREFIX:      &str = "fraise:email-verify:";
const VERIFY_TTL:         u64  = 86_400;
const RESEND_RATE_PREFIX: &str = "fraise:rate:email-resend:";
const RESEND_RATE_TTL:    u64  = 300;
const MAGIC_LINK_PREFIX:  &str = "fraise:magic:";
const MAGIC_LINK_TTL:     u64  = 900;
const MAGIC_RATE_PREFIX:  &str = "fraise:rate:magic:";
const MAGIC_RATE_TTL:     u64  = 120;
const RESET_RATE_PREFIX:  &str = "fraise:rate:reset:";
const RESET_RATE_TTL:     u64  = 300;

// ── Apple Sign In ─────────────────────────────────────────────────────────────

pub async fn authenticate_apple(
    pool:           &PgPool,
    cfg:            &Arc<Config>,
    http:           &reqwest::Client,
    identity_token: &str,
    display_name:   Option<&str>,
) -> AppResult<AuthResponse> {
    let claims = crate::auth::apple::verify_identity_token(identity_token, cfg, http).await?;

    let email = claims.email.as_deref();
    let (user, is_new) =
        repository::find_or_create_apple(pool, &claims.sub, email, display_name).await?;

    if user.banned {
        return Err(DomainError::Forbidden);
    }

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new, verified: user.verified })
}

// ── Demo (Apple App Review) ───────────────────────────────────────────────────

pub async fn authenticate_demo(
    pool: &PgPool,
    cfg:  &Arc<Config>,
    pin:  &str,
    ip:   Option<IpAddr>,
) -> AppResult<AuthResponse> {
    let expected = cfg
        .review_pin
        .as_ref()
        .map(|s| s.expose_secret())
        .ok_or(DomainError::Unauthorized)?;

    if !constant_time_eq(pin.as_bytes(), expected.as_bytes()) {
        audit::write(pool, None, None, "auth.demo_login_failed",
            serde_json::json!({ "reason": "invalid_pin" }), ip).await;
        return Err(DomainError::Unauthorized);
    }

    let user = repository::find_by_email(pool, "demo@fraise.box")
        .await?
        .ok_or(DomainError::Unauthorized)?;

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: user.verified })
}

// ── Email + password ──────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn register_user(
    pool:         &PgPool,
    cfg:          &Arc<Config>,
    http:         &reqwest::Client,
    redis:        Option<&deadpool_redis::Pool>,
    email:        &str,
    password:     &str,
    display_name: Option<&str>,
    event_bus:    &EventBus,
) -> AppResult<AuthResponse> {
    let hash = bcrypt::hash(password, 10)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    let user = repository::create_email_user(pool, email, &hash, display_name).await?;

    event_bus.publish(DomainEvent::UserRegistered {
        user_id: user.id,
        email:   email.to_owned(),
    });

    if let Some(api_key) = cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned()) {
        let http    = http.clone();
        let base    = cfg.api_base_url.clone();
        let user_id = user.id;
        let to      = email.to_owned();
        let redis   = redis.cloned();
        tokio::spawn(async move {
            if let Some(verify_url) = issue_verification_token(redis.as_ref(), user_id, &base).await {
                if let Err(e) = resend::send_verification_email(&http, &api_key, &to, &verify_url).await {
                    tracing::error!(user_id = %user_id, error = %e, "verification email delivery failed");
                }
            }
        });
    }

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: true, verified: user.verified })
}

pub async fn login_user(
    pool:     &PgPool,
    cfg:      &Arc<Config>,
    email:    &str,
    password: &str,
    ip:       Option<IpAddr>,
) -> AppResult<AuthResponse> {
    let user = match repository::find_by_email(pool, email).await? {
        Some(u) => u,
        None => {
            audit::write(pool, None, None, "auth.login_failed",
                serde_json::json!({ "reason": "user_not_found" }), ip).await;
            return Err(DomainError::Unauthorized);
        }
    };

    if user.banned {
        audit::write(pool, Some(user.id.into()), None, "auth.login_blocked",
            serde_json::json!({ "reason": "banned" }), ip).await;
        return Err(DomainError::Forbidden);
    }

    let hash = user.password_hash.as_deref().ok_or(DomainError::Unauthorized)?;
    let valid = bcrypt::verify(password, hash)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    if !valid {
        audit::write(pool, Some(user.id.into()), None, "auth.login_failed",
            serde_json::json!({ "reason": "invalid_password" }), ip).await;
        return Err(DomainError::Unauthorized);
    }

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: user.verified })
}

// ── Password reset ────────────────────────────────────────────────────────────

pub async fn request_password_reset(
    pool:  &PgPool,
    cfg:   &Arc<Config>,
    http:  &reqwest::Client,
    redis: Option<&deadpool_redis::Pool>,
    email: &str,
) -> AppResult<()> {
    if let Some(pool_r) = redis {
        let key = format!("{RESET_RATE_PREFIX}{}", email.to_lowercase());
        let mut conn = pool_r.get().await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;
        let count: i64 = redis::cmd("INCR").arg(&key)
            .query_async(&mut *conn).await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;
        if count == 1 {
            let _: () = redis::cmd("EXPIRE").arg(&key).arg(RESET_RATE_TTL)
                .query_async(&mut *conn).await.unwrap_or(());
        }
        if count > 1 { return Ok(()); }
    }

    if let Some(user) = repository::find_by_email(pool, email).await? {
        let token = Uuid::new_v4().to_string();
        repository::create_reset_token(pool, user.id, &token).await?;

        if let Some(api_key) = cfg.resend_api_key.as_ref().map(|k| k.expose_secret().to_owned()) {
            let http     = http.clone();
            let base_url = cfg.api_base_url.clone();
            let to       = email.to_owned();
            tokio::spawn(async move {
                let reset_url = format!("{base_url}/reset-password?token={token}");
                if let Err(e) = resend::send_password_reset(&http, &api_key, &to, &reset_url).await {
                    tracing::error!(error = %e, "password reset email delivery failed");
                }
            });
        }
    }
    Ok(())
}

pub async fn reset_password(pool: &PgPool, token: &str, new_password: &str) -> AppResult<()> {
    let user_id = repository::consume_reset_token(pool, token)
        .await?
        .ok_or_else(|| DomainError::invalid_input("invalid or expired reset token"))?;

    let hash = bcrypt::hash(new_password, 10)
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    repository::set_password(pool, user_id, &hash).await
}

// ── Require active user ───────────────────────────────────────────────────────

pub async fn get_active_user(pool: &PgPool, user_id: UserId) -> AppResult<UserRow> {
    let user = repository::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::Unauthorized)?;

    if user.banned { return Err(DomainError::Forbidden); }
    Ok(user)
}

// ── Magic link auth ───────────────────────────────────────────────────────────

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
    if user.banned { return Ok(()); }

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

pub async fn verify_magic_link(
    pool:  &PgPool,
    cfg:   &Arc<Config>,
    redis: Option<&deadpool_redis::Pool>,
    token: &str,
    ip:    Option<IpAddr>,
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

    if user.banned {
        audit::write(pool, Some(user_id.into()), None, "auth.login_blocked",
            serde_json::json!({ "reason": "banned", "via": "magic_link" }), ip).await;
        return Err(DomainError::Forbidden);
    }

    if !user.verified {
        repository::set_verified(pool, user_id).await?;
    }

    let jwt = auth::sign_token(user_id, cfg)?;
    Ok(AuthResponse { user_id, token: jwt, is_new: false, verified: true })
}

// ── Email verification ────────────────────────────────────────────────────────

pub async fn verify_email(
    pool:  &PgPool,
    redis: Option<&deadpool_redis::Pool>,
    token: &str,
) -> AppResult<String> {
    let redis_pool = redis.ok_or(DomainError::Unauthorized)?;

    let key = format!("{VERIFY_PREFIX}{token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    let user_id_str: Option<String> = redis::cmd("GETDEL").arg(&key)
        .query_async(&mut *conn).await
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let user_id_raw = user_id_str
        .ok_or(DomainError::Unauthorized)?
        .parse::<i32>()
        .map_err(|_| DomainError::Unauthorized)?;

    let user_id = UserId::from(user_id_raw);
    repository::set_verified(pool, user_id).await?;

    let user = repository::find_by_id(pool, user_id).await?.ok_or(DomainError::NotFound)?;

    audit::write(pool, Some(user_id_raw), None, "auth.email_verified",
        serde_json::json!({ "email": user.email }), None).await;

    Ok(user.email)
}

async fn resend_verification(
    cfg:     &Arc<Config>,
    http:    &reqwest::Client,
    redis:   Option<&deadpool_redis::Pool>,
    user_id: UserId,
    email:   &str,
) -> AppResult<()> {
    if let Some(redis_pool) = redis {
        let key = format!("{RESEND_RATE_PREFIX}{}", i32::from(user_id));
        let mut conn = redis_pool.get().await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis: {e}")))?;

        let count: i64 = redis::cmd("INCR").arg(&key)
            .query_async(&mut *conn).await
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;

        if count == 1 {
            let _: () = redis::cmd("EXPIRE").arg(&key).arg(RESEND_RATE_TTL)
                .query_async(&mut *conn).await.unwrap_or(());
        }
        if count > 1 {
            return Err(DomainError::Unprocessable(
                "please wait a few minutes before requesting another verification email".into(),
            ));
        }
    }

    let api_key = cfg.resend_api_key.as_ref()
        .map(|k| k.expose_secret().to_owned())
        .ok_or_else(|| DomainError::Internal(anyhow::anyhow!("email not configured")))?;

    if let Some(verify_url) = issue_verification_token(redis, user_id, &cfg.api_base_url).await {
        let _ = resend::send_verification_email(http, &api_key, email, &verify_url).await;
    }

    Ok(())
}

// ── Profile mutations ─────────────────────────────────────────────────────────

pub async fn update_push_token(pool: &PgPool, user_id: UserId, token: &str) -> AppResult<()> {
    repository::set_push_token(pool, user_id, token).await
}

pub async fn update_display_name(pool: &PgPool, user_id: UserId, name: &str) -> AppResult<()> {
    repository::set_display_name(pool, user_id, name).await
}

// QUERY — no side effects
pub async fn is_user_verified(pool: &PgPool, user_id: UserId) -> AppResult<bool> {
    let user = repository::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::NotFound)?;
    Ok(user.verified)
}

// COMMAND — sends verification email; caller must check is_user_verified first
pub async fn resend_verification_email(
    pool:    &PgPool,
    cfg:     &Arc<Config>,
    http:    &reqwest::Client,
    redis:   Option<&deadpool_redis::Pool>,
    user_id: UserId,
) -> AppResult<()> {
    let user = repository::find_by_id(pool, user_id)
        .await?
        .ok_or(DomainError::NotFound)?;
    resend_verification(cfg, http, redis, user_id, &user.email).await
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use ring::{
        hmac::{self, Key, HMAC_SHA256},
        rand::{SecureRandom, SystemRandom},
    };
    let rng = SystemRandom::new();
    let mut key_bytes = [0u8; 32];
    if rng.fill(&mut key_bytes).is_err() { return false; }
    let key   = Key::new(HMAC_SHA256, &key_bytes);
    let mac_a = hmac::sign(&key, a);
    let mac_b = hmac::sign(&key, b);
    mac_a.as_ref().iter().zip(mac_b.as_ref())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn issue_verification_token(
    redis:    Option<&deadpool_redis::Pool>,
    user_id:  UserId,
    base_url: &str,
) -> Option<String> {
    let pool  = redis?;
    let token = Uuid::new_v4().to_string();
    let key   = format!("{VERIFY_PREFIX}{token}");

    let mut conn = pool.get().await.ok()?;
    let _: redis::Value = redis::cmd("SET")
        .arg(&key).arg(i32::from(user_id).to_string())
        .arg("EX").arg(VERIFY_TTL).arg("NX")
        .query_async(&mut *conn).await.ok()?;

    Some(format!("{base_url}/api/auth/verify-email?token={token}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::error::DomainError;
    use crate::event_bus::EventBus;
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

    #[sqlx::test(migrations = "../server/migrations")]
    async fn register_user_creates_user_in_db(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        let resp = register_user(&pool, &cfg, &http, None, "new@test.com", "password123", None, &bus)
            .await
            .unwrap();

        assert!(resp.is_new, "first registration must have is_new=true");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
            .bind("new@test.com")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn register_user_duplicate_email_is_conflict(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        register_user(&pool, &cfg, &http, None, "dup@test.com", "pass1", None, &bus)
            .await
            .unwrap();

        let result = register_user(&pool, &cfg, &http, None, "dup@test.com", "pass2", None, &bus).await;
        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "duplicate email must return Conflict, got: {result:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn login_user_correct_password_succeeds(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        register_user(&pool, &cfg, &http, None, "login@test.com", "secret99", None, &bus)
            .await
            .unwrap();

        let resp = login_user(&pool, &cfg, "login@test.com", "secret99", None)
            .await
            .unwrap();
        assert!(!resp.token.is_empty());
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn login_user_wrong_password_is_unauthorized(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        register_user(&pool, &cfg, &http, None, "wrongpw@test.com", "correct99", None, &bus)
            .await
            .unwrap();

        let result = login_user(&pool, &cfg, "wrongpw@test.com", "wrong999", None).await;
        assert!(
            matches!(result, Err(DomainError::Unauthorized)),
            "wrong password must return Unauthorized, got: {result:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn login_user_banned_user_is_forbidden(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        let resp = register_user(&pool, &cfg, &http, None, "banned@test.com", "pass1234", None, &bus)
            .await
            .unwrap();

        sqlx::query("UPDATE users SET banned = true WHERE id = $1")
            .bind(i32::from(resp.user_id))
            .execute(&pool)
            .await
            .unwrap();

        let result = login_user(&pool, &cfg, "banned@test.com", "pass1234", None).await;
        assert!(
            matches!(result, Err(DomainError::Forbidden)),
            "banned user must return Forbidden, got: {result:?}"
        );
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn request_password_reset_creates_token_in_db(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        let bus  = EventBus::new();

        register_user(&pool, &cfg, &http, None, "reset@test.com", "pass1234", None, &bus)
            .await
            .unwrap();

        request_password_reset(&pool, &cfg, &http, None, "reset@test.com")
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "one reset token must be created");
    }

    #[sqlx::test(migrations = "../server/migrations")]
    async fn request_password_reset_unknown_email_is_silent(pool: PgPool) {
        let cfg  = test_cfg();
        let http = reqwest::Client::new();
        // Must succeed silently — never reveal whether email exists.
        request_password_reset(&pool, &cfg, &http, None, "ghost@test.com")
            .await
            .unwrap();
    }
}
