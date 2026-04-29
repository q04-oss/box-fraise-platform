use uuid::Uuid;

use crate::{
    auth,
    app::AppState,
    error::{AppError, AppResult},
};
use super::{
    repository,
    types::{AuthResponse, UserRow},
};

// ── Apple Sign In ─────────────────────────────────────────────────────────────

pub async fn apple_sign_in(
    state:          &AppState,
    identity_token: &str,
    display_name:   Option<&str>,
) -> AppResult<AuthResponse> {
    let claims = crate::auth::apple::verify_identity_token(
        identity_token,
        &state.cfg,
        &state.http,
    )
    .await?;

    let email = claims.email.as_deref();
    let (user, is_new) =
        repository::find_or_create_apple(&state.db, &claims.sub, email, display_name).await?;

    if user.banned {
        return Err(AppError::Forbidden);
    }

    if is_new {
        if let Some(email) = claims.email {
            let pool = state.db.clone();
            let uid  = user.id;
            tokio::spawn(async move {
                repository::maybe_verify_from_booking(&pool, uid, &email).await;
            });
        }
    }

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new, verified: user.verified })
}

// ── Operator login ────────────────────────────────────────────────────────────

pub async fn operator_login(
    state:       &AppState,
    code:        &str,
    location_id: i32,
) -> AppResult<AuthResponse> {
    let user = repository::find_operator(&state.db, code, location_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: true })
}

// ── Demo (Apple App Review) ───────────────────────────────────────────────────

pub async fn demo_login(state: &AppState, pin: &str) -> AppResult<AuthResponse> {
    let expected = state.cfg.review_pin.as_deref().ok_or(AppError::Unauthorized)?;

    // Constant-time comparison prevents timing oracle on the PIN.
    if !constant_time_eq(pin.as_bytes(), expected.as_bytes()) {
        return Err(AppError::Unauthorized);
    }

    let user = repository::find_by_email(&state.db, "demo@fraise.box")
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: user.verified })
}

// ── Email + password ──────────────────────────────────────────────────────────

pub async fn register(
    state:        &AppState,
    email:        &str,
    password:     &str,
    display_name: Option<&str>,
) -> AppResult<AuthResponse> {
    let hash = bcrypt::hash(password, 10)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    // create_email_user uses INSERT ON CONFLICT DO NOTHING, so duplicate emails
    // return AppError::Conflict rather than a DB error.
    let user = repository::create_email_user(&state.db, email, &hash, display_name).await?;

    // TODO: send welcome email via integrations::resend

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: true, verified: user.verified })
}

pub async fn login(state: &AppState, email: &str, password: &str) -> AppResult<AuthResponse> {
    let user = repository::find_by_email(&state.db, email)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.banned {
        return Err(AppError::Forbidden);
    }

    let hash = user.password_hash.as_deref().ok_or(AppError::Unauthorized)?;
    let valid = bcrypt::verify(password, hash)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    if !valid {
        return Err(AppError::Unauthorized);
    }

    let token = auth::sign_token(user.id, &state.cfg)?;
    Ok(AuthResponse { user_id: user.id, token, is_new: false, verified: user.verified })
}

// ── Password reset ────────────────────────────────────────────────────────────

pub async fn forgot_password(state: &AppState, email: &str) -> AppResult<()> {
    if let Some(user) = repository::find_by_email(&state.db, email).await? {
        let token = Uuid::new_v4().to_string();
        repository::create_reset_token(&state.db, user.id, &token).await?;
        // TODO: integrations::resend::send_password_reset(&state.http, &cfg, email, &token)
        tracing::info!(user_id = user.id, "password reset token generated");
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
pub async fn require_active(state: &AppState, user_id: i32) -> AppResult<UserRow> {
    let user = repository::find_by_id(&state.db, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.banned {
        return Err(AppError::Forbidden);
    }

    Ok(user)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
