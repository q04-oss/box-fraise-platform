use deadpool_redis::redis;
use secrecy::ExposeSecret;
use sqlx::PgPool;
use std::{net::IpAddr, sync::Arc};
use uuid::Uuid;

use crate::{
    audit, auth,
    config::Config,
    error::{AppError, AppResult},
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

pub async fn apple_sign_in(
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
        return Err(AppError::Forbidden);
    }

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new, verified: user.verified })
}

// ── Demo (Apple App Review) ───────────────────────────────────────────────────

pub async fn demo_login(
    pool: &PgPool,
    cfg:  &Arc<Config>,
    pin:  &str,
    ip:   Option<IpAddr>,
) -> AppResult<AuthResponse> {
    let expected = cfg
        .review_pin
        .as_ref()
        .map(|s| s.expose_secret())
        .ok_or(AppError::Unauthorized)?;

    if !constant_time_eq(pin.as_bytes(), expected.as_bytes()) {
        audit::write(pool, None, None, "auth.demo_login_failed",
            serde_json::json!({ "reason": "invalid_pin" }), ip).await;
        return Err(AppError::Unauthorized);
    }

    let user = repository::find_by_email(pool, "demo@fraise.box")
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: user.verified })
}

// ── Email + password ──────────────────────────────────────────────────────────

pub async fn register(
    pool:         &PgPool,
    cfg:          &Arc<Config>,
    http:         &reqwest::Client,
    redis:        Option<&deadpool_redis::Pool>,
    email:        &str,
    password:     &str,
    display_name: Option<&str>,
) -> AppResult<AuthResponse> {
    let hash = bcrypt::hash(password, 10)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    let user = repository::create_email_user(pool, email, &hash, display_name).await?;

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

pub async fn login(
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
            return Err(AppError::Unauthorized);
        }
    };

    if user.banned {
        audit::write(pool, Some(user.id.into()), None, "auth.login_blocked",
            serde_json::json!({ "reason": "banned" }), ip).await;
        return Err(AppError::Forbidden);
    }

    let hash = user.password_hash.as_deref().ok_or(AppError::Unauthorized)?;
    let valid = bcrypt::verify(password, hash)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    if !valid {
        audit::write(pool, Some(user.id.into()), None, "auth.login_failed",
            serde_json::json!({ "reason": "invalid_password" }), ip).await;
        return Err(AppError::Unauthorized);
    }

    let token = auth::sign_token(user.id, cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: user.verified })
}

// ── Password reset ────────────────────────────────────────────────────────────

pub async fn forgot_password(
    pool:  &PgPool,
    cfg:   &Arc<Config>,
    http:  &reqwest::Client,
    redis: Option<&deadpool_redis::Pool>,
    email: &str,
) -> AppResult<()> {
    if let Some(pool_r) = redis {
        let key = format!("{RESET_RATE_PREFIX}{}", email.to_lowercase());
        let mut conn = pool_r.get().await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
        let count: i64 = redis::cmd("INCR").arg(&key)
            .query_async(&mut *conn).await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;
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
        .ok_or_else(|| AppError::bad_request("invalid or expired reset token"))?;

    let hash = bcrypt::hash(new_password, 10)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    repository::set_password(pool, user_id, &hash).await
}

// ── Require active user ───────────────────────────────────────────────────────

pub async fn require_active(pool: &PgPool, user_id: UserId) -> AppResult<UserRow> {
    let user = repository::find_by_id(pool, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.banned { return Err(AppError::Forbidden); }
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
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
        let count: i64 = redis::cmd("INCR").arg(&key)
            .query_async(&mut *conn).await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;
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
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    let _: () = redis::cmd("SET")
        .arg(&key).arg(i32::from(user.id).to_string())
        .arg("EX").arg(MAGIC_LINK_TTL).arg("NX")
        .query_async(&mut *conn).await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis SET: {e}")))?;

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
    let redis_pool = redis.ok_or(AppError::Unauthorized)?;

    let key = format!("{MAGIC_LINK_PREFIX}{token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    let raw: Option<String> = redis::cmd("GETDEL").arg(&key)
        .query_async(&mut *conn).await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let user_id = UserId::from(match raw {
        Some(s) => s.parse::<i32>().map_err(|_| AppError::Unauthorized)?,
        None => {
            audit::write(pool, None, None, "auth.magic_link_invalid",
                serde_json::json!({ "reason": "token_expired_or_consumed" }), ip).await;
            return Err(AppError::Unauthorized);
        }
    });

    let user = repository::find_by_id(pool, user_id).await?.ok_or(AppError::Unauthorized)?;

    if user.banned {
        audit::write(pool, Some(user_id.into()), None, "auth.login_blocked",
            serde_json::json!({ "reason": "banned", "via": "magic_link" }), ip).await;
        return Err(AppError::Forbidden);
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
    let redis_pool = redis.ok_or(AppError::Unauthorized)?;

    let key = format!("{VERIFY_PREFIX}{token}");
    let mut conn = redis_pool.get().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    let user_id_str: Option<String> = redis::cmd("GETDEL").arg(&key)
        .query_async(&mut *conn).await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let user_id_raw = user_id_str
        .ok_or(AppError::Unauthorized)?
        .parse::<i32>()
        .map_err(|_| AppError::Unauthorized)?;

    let user_id = UserId::from(user_id_raw);
    repository::set_verified(pool, user_id).await?;

    let user = repository::find_by_id(pool, user_id).await?.ok_or(AppError::NotFound)?;

    audit::write(pool, Some(user_id_raw), None, "auth.email_verified",
        serde_json::json!({ "email": user.email }), None).await;

    Ok(user.email)
}

pub async fn resend_verification(
    cfg:     &Arc<Config>,
    http:    &reqwest::Client,
    redis:   Option<&deadpool_redis::Pool>,
    user_id: UserId,
    email:   &str,
) -> AppResult<()> {
    if let Some(redis_pool) = redis {
        let key = format!("{RESEND_RATE_PREFIX}{}", i32::from(user_id));
        let mut conn = redis_pool.get().await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

        let count: i64 = redis::cmd("INCR").arg(&key)
            .query_async(&mut *conn).await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;

        if count == 1 {
            let _: () = redis::cmd("EXPIRE").arg(&key).arg(RESEND_RATE_TTL)
                .query_async(&mut *conn).await.unwrap_or(());
        }
        if count > 1 {
            return Err(AppError::Unprocessable(
                "please wait a few minutes before requesting another verification email".into(),
            ));
        }
    }

    let api_key = cfg.resend_api_key.as_ref()
        .map(|k| k.expose_secret().to_owned())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("email not configured")))?;

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

/// Looks up the user, checks whether they are already verified, and re-issues
/// a verification email if not. Returns `true` when the user was already
/// verified (the caller can surface this to the client).
pub async fn request_resend_verification(
    pool:    &PgPool,
    cfg:     &Arc<Config>,
    http:    &reqwest::Client,
    redis:   Option<&deadpool_redis::Pool>,
    user_id: UserId,
) -> AppResult<bool> {
    let user = repository::find_by_id(pool, user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if user.verified {
        return Ok(true);
    }

    resend_verification(cfg, http, redis, user_id, &user.email).await?;
    Ok(false)
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
