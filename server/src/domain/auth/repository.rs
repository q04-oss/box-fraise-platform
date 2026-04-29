use rand::Rng;
use sqlx::PgPool;

use crate::{error::{AppError, AppResult}, types::UserId};
use super::types::{UserRow, USER_COLS};

// ── Lookups ───────────────────────────────────────────────────────────────────

pub async fn find_by_id(pool: &PgPool, id: UserId) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!("SELECT {USER_COLS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE LOWER(email) = LOWER($1)"
    ))
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Find or create via Apple Sign In ─────────────────────────────────────────

/// Returns `(user, is_new)`. Uses an atomic UPSERT so concurrent sign-ins
/// with the same Apple ID produce exactly one row.
pub async fn find_or_create_apple(
    pool:         &PgPool,
    apple_id:     &str,
    email:        Option<&str>,
    display_name: Option<&str>,
) -> AppResult<(UserRow, bool)> {
    let mut tx = pool.begin().await.map_err(AppError::Db)?;

    // 1. Look up by Apple user ID first — fastest path for returning users.
    let existing: Option<UserRow> = sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE apple_user_id = $1 FOR UPDATE"
    ))
    .bind(apple_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    if let Some(user) = existing {
        tx.commit().await.map_err(AppError::Db)?;
        return Ok((user, false));
    }

    // 2. Atomically insert or link to an existing email account.
    // ON CONFLICT (email) links the Apple ID to an existing email user.
    let email_str = email
        .map(String::from)
        .unwrap_or_else(|| format!("{apple_id}@privaterelay.appleid.com"));

    let user_code = generate_unique_code_tx(&mut tx).await?;

    let user: UserRow = sqlx::query_as(&format!(
        "INSERT INTO users (apple_user_id, email, display_name, user_code)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO UPDATE
         SET apple_user_id = EXCLUDED.apple_user_id
         RETURNING {USER_COLS}"
    ))
    .bind(apple_id)
    .bind(&email_str)
    .bind(display_name)
    .bind(&user_code)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Db)?;

    tx.commit().await.map_err(AppError::Db)?;

    let is_new = email.is_none() || email_str.ends_with("@privaterelay.appleid.com");

    Ok((user, is_new))
}

/// Auto-verify `table_verified` if the user's email matches a confirmed table booking.
/// Runs fire-and-forget; failures are logged, not propagated.
pub async fn maybe_verify_from_booking(pool: &PgPool, user_id: UserId, email: &str) {
    let result = sqlx::query(
        "UPDATE users
         SET table_verified = true
         WHERE id = $1
           AND table_verified = false
           AND EXISTS (
               SELECT 1 FROM table_bookings
               WHERE LOWER(email) = LOWER($2)
                 AND status = 'confirmed'
           )",
    )
    .bind(user_id)
    .bind(email)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(user_id = %user_id, error = %e, "maybe_verify_from_booking failed");
    }
}

// ── Operator login ────────────────────────────────────────────────────────────

pub async fn find_operator(pool: &PgPool, code: &str, location_id: i32) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS}
         FROM users
         WHERE id = (
             SELECT u.id
             FROM locations l
             JOIN users u ON u.business_id = l.business_id AND u.is_shop = true
             WHERE l.staff_pin = $1 AND l.id = $2
             LIMIT 1
         )"
    ))
    .bind(code)
    .bind(location_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Email + password auth ─────────────────────────────────────────────────────

pub async fn create_email_user(
    pool:          &PgPool,
    email:         &str,
    password_hash: &str,
    display_name:  Option<&str>,
) -> AppResult<UserRow> {
    let user_code = generate_unique_code(pool).await?;

    // INSERT ... ON CONFLICT DO NOTHING followed by a SELECT handles the rare
    // race where two requests create the same email simultaneously.
    let row: Option<UserRow> = sqlx::query_as(&format!(
        "INSERT INTO users (email, password_hash, display_name, user_code)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO NOTHING
         RETURNING {USER_COLS}"
    ))
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .bind(&user_code)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    // If ON CONFLICT fired, the email already exists — return Conflict.
    row.ok_or_else(|| AppError::conflict("email already in use"))
}

pub async fn set_password(pool: &PgPool, user_id: UserId, password_hash: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

// ── Profile mutations ─────────────────────────────────────────────────────────

pub async fn set_push_token(pool: &PgPool, user_id: UserId, token: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET push_token = $1 WHERE id = $2")
        .bind(token)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

pub async fn set_display_name(pool: &PgPool, user_id: UserId, name: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
        .bind(name)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

// ── Password reset tokens ─────────────────────────────────────────────────────

pub async fn create_reset_token(pool: &PgPool, user_id: UserId, token: &str) -> AppResult<()> {
    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    sqlx::query(
        "INSERT INTO password_reset_tokens (user_id, token, expires_at)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id) DO UPDATE
         SET token = EXCLUDED.token, expires_at = EXCLUDED.expires_at",
    )
    .bind(user_id)
    .bind(token)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn consume_reset_token(pool: &PgPool, token: &str) -> AppResult<Option<UserId>> {
    let row: Option<(UserId,)> = sqlx::query_as(
        "DELETE FROM password_reset_tokens
         WHERE token = $1 AND expires_at > NOW()
         RETURNING user_id",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(row.map(|(id,)| id))
}

// ── Table booking claim ───────────────────────────────────────────────────────

pub async fn claim_booking_email(pool: &PgPool, user_id: UserId, email: &str) -> AppResult<bool> {
    let matched: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM table_bookings
             WHERE LOWER(email) = LOWER($1) AND status = 'confirmed'
         )",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    if matched {
        sqlx::query("UPDATE users SET table_verified = true WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(AppError::Db)?;
    }

    Ok(matched)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Generate a unique 6-character user code, retrying on collision up to 10 times.
async fn generate_unique_code(pool: &PgPool) -> AppResult<String> {
    for _ in 0..10 {
        let code = random_code();
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE user_code = $1)")
                .bind(&code)
                .fetch_one(pool)
                .await
                .map_err(AppError::Db)?;
        if !exists {
            return Ok(code);
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "could not generate a unique user_code after 10 attempts"
    )))
}

/// Same as `generate_unique_code` but operates inside an existing transaction.
async fn generate_unique_code_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> AppResult<String> {
    for _ in 0..10 {
        let code = random_code();
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE user_code = $1)")
                .bind(&code)
                .fetch_one(&mut **tx)
                .await
                .map_err(AppError::Db)?;
        if !exists {
            return Ok(code);
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "could not generate a unique user_code after 10 attempts"
    )))
}

/// Excludes visually ambiguous characters (0/O, 1/I/L).
fn random_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}
