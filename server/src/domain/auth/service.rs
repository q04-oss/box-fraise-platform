use deadpool_redis::redis;
use secrecy::ExposeSecret;
use std::net::IpAddr;
use uuid::Uuid;

use crate::{
    app::AppState,
    audit, auth,
    auth::staff,
    error::{AppError, AppResult},
    integrations::resend,
    types::UserId,
};

const VERIFY_PREFIX: &str = "fraise:email-verify:";
const VERIFY_TTL: u64 = 86_400; // 24 hours
const RESEND_RATE_PREFIX: &str = "fraise:rate:email-resend:";
const RESEND_RATE_TTL: u64 = 300; // 5 minutes — one resend per window
const MAGIC_LINK_PREFIX: &str = "fraise:magic:";
const MAGIC_LINK_TTL: u64 = 900; // 15 minutes
const MAGIC_RATE_PREFIX: &str = "fraise:rate:magic:";
const MAGIC_RATE_TTL: u64 = 120; // 2 minutes between requests per email
const RESET_RATE_PREFIX: &str = "fraise:rate:reset:";
const RESET_RATE_TTL: u64 = 300; // 5 minutes between reset requests per email
use super::{
    repository,
    types::{AuthResponse, StaffAuthResponse, UserRow},
};

// ── Apple Sign In ─────────────────────────────────────────────────────────────

pub async fn apple_sign_in(
    state: &AppState,
    identity_token: &str,
    display_name: Option<&str>,
) -> AppResult<AuthResponse> {
    let claims =
        crate::auth::apple::verify_identity_token(identity_token, &state.cfg, &state.http).await?;

    let email = claims.email.as_deref();
    let (user, is_new) =
        repository::find_or_create_apple(&state.db, &claims.sub, email, display_name).await?;

    if user.banned {
        return Err(AppError::Forbidden);
    }

    if is_new {
        if let Some(email) = claims.email {
            let pool = state.db.clone();
            let uid = user.id;
            tokio::spawn(async move {
                repository::maybe_verify_from_booking(&pool, uid, &email).await;
            });
        }
    }

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse {
        user_id: user.id,
        token,
        is_new,
        verified: user.verified,
    })
}

// ── Operator login ────────────────────────────────────────────────────────────

pub async fn operator_login(
    state: &AppState,
    code: &str,
    location_id: i32,
) -> AppResult<AuthResponse> {
    let user = repository::find_operator(&state.db, code, location_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse {
        user_id: user.id,
        token,
        is_new: false,
        verified: true,
    })
}

// ── Staff login ───────────────────────────────────────────────────────────────

/// Authenticates a staff member against a location's staff PIN and issues a
/// `StaffClaims` JWT scoped to that location's business.
///
/// The issued token is short-lived (8h / one shift) and signed with
/// `STAFF_JWT_SECRET` — cryptographically distinct from user tokens.
pub async fn staff_login(
    state: &AppState,
    pin: &str,
    location_id: i32,
    ip: Option<IpAddr>,
) -> AppResult<StaffAuthResponse> {
    let (user_id, business_id) = repository::find_staff_with_business(&state.db, pin, location_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = staff::sign_staff_token(user_id, business_id, &state.cfg)?;

    audit::write(
        &state.db,
        Some(user_id.into()),
        Some(business_id),
        "auth.staff_login",
        serde_json::json!({ "location_id": location_id }),
        ip,
    )
    .await;

    Ok(StaffAuthResponse {
        user_id: user_id.into(),
        business_id,
        token,
    })
}

// ── Demo (Apple App Review) ───────────────────────────────────────────────────

pub async fn demo_login(
    state: &AppState,
    pin: &str,
    ip: Option<IpAddr>,
) -> AppResult<AuthResponse> {
    let expected = state
        .cfg
        .review_pin
        .as_ref()
        .map(|s| s.expose_secret())
        .ok_or(AppError::Unauthorized)?;

    if !constant_time_eq(pin.as_bytes(), expected.as_bytes()) {
        audit::write(
            &state.db,
            None,
            None,
            "auth.demo_login_failed",
            serde_json::json!({ "reason": "invalid_pin" }),
            ip,
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    let user = repository::find_by_email(&state.db, "demo@fraise.box")
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse {
        user_id: user.id,
        token,
        is_new: false,
        verified: user.verified,
    })
}

// ── Email + password ──────────────────────────────────────────────────────────

pub async fn register(
    state: &AppState,
    email: &str,
    password: &str,
    display_name: Option<&str>,
) -> AppResult<AuthResponse> {
    let hash = bcrypt::hash(password, 10)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    // create_email_user uses INSERT ON CONFLICT DO NOTHING, so duplicate emails
    // return AppError::Conflict rather than a DB error.
    let user = repository::create_email_user(&state.db, email, &hash, display_name).await?;

    // Send verification email. Fire-and-forget — a failed email never blocks
    // registration. The user can request a resend from within the app.
    if let Some(api_key) = state
        .cfg
        .resend_api_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned())
    {
        let http = state.http.clone();
        let base_url = state.cfg.api_base_url.clone();
        let user_id = user.id;
        let user_email = email.to_owned();
        let redis = state.redis.clone();
        tokio::spawn(async move {
            if let Some(verify_url) =
                issue_verification_token(redis.as_ref(), user_id, &base_url).await
            {
                if let Err(e) = resend::send_verification_email(&http, &api_key, &user_email, &verify_url).await {
                    tracing::error!(user_id = %user_id, error = %e, "verification email delivery failed");
                }
            }
        });
    }

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse {
        user_id: user.id,
        token,
        is_new: true,
        verified: user.verified,
    })
}

pub async fn login(
    state: &AppState,
    email: &str,
    password: &str,
    ip: Option<IpAddr>,
) -> AppResult<AuthResponse> {
    let user = match repository::find_by_email(&state.db, email).await? {
        Some(u) => u,
        None => {
            audit::write(
                &state.db,
                None,
                None,
                "auth.login_failed",
                serde_json::json!({ "reason": "user_not_found" }),
                ip,
            )
            .await;
            return Err(AppError::Unauthorized);
        }
    };

    if user.banned {
        audit::write(
            &state.db,
            Some(user.id.into()),
            None,
            "auth.login_blocked",
            serde_json::json!({ "reason": "banned" }),
            ip,
        )
        .await;
        return Err(AppError::Forbidden);
    }

    let hash = user
        .password_hash
        .as_deref()
        .ok_or(AppError::Unauthorized)?;
    let valid = bcrypt::verify(password, hash)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    if !valid {
        audit::write(
            &state.db,
            Some(user.id.into()),
            None,
            "auth.login_failed",
            serde_json::json!({ "reason": "invalid_password" }),
            ip,
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse {
        user_id: user.id,
        token,
        is_new: false,
        verified: user.verified,
    })
}

// ── Password reset ────────────────────────────────────────────────────────────

pub async fn forgot_password(state: &AppState, email: &str) -> AppResult<()> {
    // Per-email rate limit — same pattern as magic link. Silent on excess so
    // the response is identical whether the email exists or not (prevents enumeration
    // and also prevents using 429 as a signal that the email is registered).
    if let Some(pool) = state.redis.as_ref() {
        let key = format!("{RESET_RATE_PREFIX}{}", email.to_lowercase());
        let mut conn = pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
        let count: i64 = redis::cmd("INCR")
            .arg(&key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;
        if count == 1 {
            let _: () = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(RESET_RATE_TTL)
                .query_async(&mut *conn)
                .await
                .unwrap_or(());
        }
        if count > 1 {
            return Ok(());
        }
    }

    if let Some(user) = repository::find_by_email(&state.db, email).await? {
        let token = Uuid::new_v4().to_string();
        repository::create_reset_token(&state.db, user.id, &token).await?;

        // Fire-and-forget — email delivery failure must not roll back the token.
        if let Some(api_key) = state
            .cfg
            .resend_api_key
            .as_ref()
            .map(|k| k.expose_secret().to_owned())
        {
            let http = state.http.clone();
            let base_url = state.cfg.api_base_url.clone();
            let to = email.to_owned();
            tokio::spawn(async move {
                // Universal Link — iOS intercepts this and shows the in-app reset form.
                let reset_url = format!("{base_url}/reset-password?token={token}");
                if let Err(e) = resend::send_password_reset(&http, &api_key, &to, &reset_url).await {
                    // No user context logged — prevents confirming whether an email exists.
                    tracing::error!(error = %e, "password reset email delivery failed");
                }
            });
        }
    }
    // Intentionally silent whether the email exists — prevents enumeration.
    Ok(())
}

pub async fn reset_password(state: &AppState, token: &str, new_password: &str) -> AppResult<()> {
    let user_id = repository::consume_reset_token(&state.db, token)
        .await?
        .ok_or_else(|| AppError::bad_request("invalid or expired reset token"))?;

    let hash = bcrypt::hash(new_password, 10)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    repository::set_password(&state.db, user_id, &hash).await
}

// ── Require active user ───────────────────────────────────────────────────────

/// Fetch and validate a user by ID. Call in handlers that need the full UserRow —
/// the `RequireUser` extractor only decodes the JWT.
pub async fn require_active(state: &AppState, user_id: UserId) -> AppResult<UserRow> {
    let user = repository::find_by_id(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.banned {
        return Err(AppError::Forbidden);
    }

    Ok(user)
}

// ── Magic link auth ───────────────────────────────────────────────────────────

/// Sends a one-time sign-in link to `email`. Creates the user if they don't
/// exist yet. Always returns Ok — never reveals whether the email is registered.
pub async fn request_magic_link(state: &AppState, email: &str) -> AppResult<()> {
    // Per-email rate limit — silent on excess to prevent enumeration.
    if let Some(pool) = state.redis.as_ref() {
        let key = format!("{MAGIC_RATE_PREFIX}{}", email.to_lowercase());
        let mut conn = pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
        let count: i64 = redis::cmd("INCR")
            .arg(&key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;
        if count == 1 {
            let _: () = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(MAGIC_RATE_TTL)
                .query_async(&mut *conn)
                .await
                .unwrap_or(());
        }
        if count > 1 {
            return Ok(());
        }
    }

    let (user, _) = repository::find_or_create_magic_link_user(&state.db, email).await?;
    if user.banned {
        return Ok(());
    }

    let Some(redis_pool) = state.redis.as_ref() else {
        return Ok(());
    };
    let token = Uuid::new_v4().to_string();
    let key = format!("{MAGIC_LINK_PREFIX}{token}");
    let mut conn = redis_pool
        .get()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(i32::from(user.id).to_string())
        .arg("EX")
        .arg(MAGIC_LINK_TTL)
        .arg("NX")
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis SET: {e}")))?;

    if let Some(api_key) = state
        .cfg
        .resend_api_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned())
    {
        let http = state.http.clone();
        let base_url = state.cfg.api_base_url.clone();
        let to = email.to_owned();
        tokio::spawn(async move {
            let link = format!("{base_url}/api/auth/magic-link/open?token={token}");
            if let Err(e) = resend::send_magic_link_email(&http, &api_key, &to, &link).await {
                tracing::error!(error = %e, "magic link email delivery failed");
            }
        });
    }

    Ok(())
}

/// Consumes a one-time magic link token and issues a JWT.
/// Marks the user's email as verified — clicking the link proves ownership.
pub async fn verify_magic_link(
    state: &AppState,
    token: &str,
    ip: Option<IpAddr>,
) -> AppResult<AuthResponse> {
    let redis_pool = state.redis.as_ref().ok_or(AppError::Unauthorized)?;

    let key = format!("{MAGIC_LINK_PREFIX}{token}");
    let mut conn = redis_pool
        .get()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    let raw: Option<String> = redis::cmd("GETDEL")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let user_id = UserId::from(match raw {
        Some(s) => s.parse::<i32>().map_err(|_| AppError::Unauthorized)?,
        None => {
            audit::write(
                &state.db,
                None,
                None,
                "auth.magic_link_invalid",
                serde_json::json!({ "reason": "token_expired_or_consumed" }),
                ip,
            )
            .await;
            return Err(AppError::Unauthorized);
        }
    });

    let user = repository::find_by_id(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.banned {
        audit::write(
            &state.db,
            Some(user_id.into()),
            None,
            "auth.login_blocked",
            serde_json::json!({ "reason": "banned", "via": "magic_link" }),
            ip,
        )
        .await;
        return Err(AppError::Forbidden);
    }

    if !user.verified {
        repository::set_verified(&state.db, user_id).await?;
    }

    let jwt = auth::sign_token(user_id, &state.cfg)?;
    Ok(AuthResponse {
        user_id,
        token: jwt,
        is_new: false,
        verified: true,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// HMAC-normalising constant-time comparison. Produces fixed-length 32-byte MACs
/// for both inputs so the final XOR-fold runs over equal-length slices regardless
/// of input length. This removes the length oracle present in naive early-return
/// implementations. Matches the implementation in admin/routes.rs.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use ring::{
        hmac::{self, Key, HMAC_SHA256},
        rand::{SecureRandom, SystemRandom},
    };
    let rng = SystemRandom::new();
    let mut key_bytes = [0u8; 32];
    if rng.fill(&mut key_bytes).is_err() {
        return false;
    }
    let key = Key::new(HMAC_SHA256, &key_bytes);
    let mac_a = hmac::sign(&key, a);
    let mac_b = hmac::sign(&key, b);
    mac_a
        .as_ref()
        .iter()
        .zip(mac_b.as_ref())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

// ── Email verification ────────────────────────────────────────────────────────

/// Consumes a verification token, marks the user verified, returns their email.
/// Returns Err(Unauthorized) if the token is expired, already used, or invalid.
pub async fn verify_email(state: &AppState, token: &str) -> AppResult<String> {
    let redis_pool = state.redis.as_ref().ok_or(AppError::Unauthorized)?;

    let key = format!("{VERIFY_PREFIX}{token}");
    let mut conn = redis_pool
        .get()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

    let user_id_str: Option<String> = redis::cmd("GETDEL")
        .arg(&key)
        .query_async(&mut *conn)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis GETDEL: {e}")))?;

    let user_id_raw = user_id_str
        .ok_or(AppError::Unauthorized)?
        .parse::<i32>()
        .map_err(|_| AppError::Unauthorized)?;

    let user_id = UserId::from(user_id_raw);
    repository::set_verified(&state.db, user_id).await?;

    let user = repository::find_by_id(&state.db, user_id)
        .await?
        .ok_or(AppError::NotFound)?;

    audit::write(
        &state.db,
        Some(user_id_raw),
        None,
        "auth.email_verified",
        serde_json::json!({ "email": user.email }),
        None,
    )
    .await;

    Ok(user.email)
}

/// Re-issues a verification token and resends the email.
/// Rate-limited to one resend per 5 minutes per user.
pub async fn resend_verification(state: &AppState, user_id: UserId, email: &str) -> AppResult<()> {
    if let Some(redis_pool) = state.redis.as_ref() {
        let key = format!("{RESEND_RATE_PREFIX}{}", i32::from(user_id));
        let mut conn = redis_pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis: {e}")))?;

        let count: i64 = redis::cmd("INCR")
            .arg(&key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Redis INCR: {e}")))?;

        if count == 1 {
            let _: () = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(RESEND_RATE_TTL)
                .query_async(&mut *conn)
                .await
                .unwrap_or(());
        }
        if count > 1 {
            return Err(AppError::Unprocessable(
                "please wait a few minutes before requesting another verification email".into(),
            ));
        }
    }

    let api_key = state
        .cfg
        .resend_api_key
        .as_ref()
        .map(|k| k.expose_secret().to_owned())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("email not configured")))?;

    if let Some(verify_url) =
        issue_verification_token(state.redis.as_ref(), user_id, &state.cfg.api_base_url).await
    {
        let _ = resend::send_verification_email(&state.http, &api_key, email, &verify_url).await;
    }

    Ok(())
}

/// Generates a verification token, stores it in Redis, returns the full URL.
/// Returns None if Redis is not configured — email verification is best-effort.
async fn issue_verification_token(
    redis: Option<&deadpool_redis::Pool>,
    user_id: UserId,
    base_url: &str,
) -> Option<String> {
    let pool = redis?;
    let token = Uuid::new_v4().to_string();
    let key = format!("{VERIFY_PREFIX}{token}");

    let mut conn = pool.get().await.ok()?;
    let _: redis::Value = redis::cmd("SET")
        .arg(&key)
        .arg(i32::from(user_id).to_string())
        .arg("EX")
        .arg(VERIFY_TTL)
        .arg("NX")
        .query_async(&mut *conn)
        .await
        .ok()?;

    Some(format!("{base_url}/api/auth/verify-email?token={token}"))
}
