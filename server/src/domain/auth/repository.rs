use rand::Rng;
use sqlx::PgPool;

use crate::error::AppResult;
use super::types::{UserRow, USER_COLS};

// ── Lookups ───────────────────────────────────────────────────────────────────

pub async fn find_by_id(pool: &PgPool, id: i32) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!("SELECT {USER_COLS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(crate::error::AppError::Db)
}

pub async fn find_by_apple_user_id(pool: &PgPool, apple_id: &str) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE apple_user_id = $1"
    ))
    .bind(apple_id)
    .fetch_optional(pool)
    .await
    .map_err(crate::error::AppError::Db)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE LOWER(email) = LOWER($1)"
    ))
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(crate::error::AppError::Db)
}

// ── Find or create via Apple Sign In ─────────────────────────────────────────

/// Returns (user, is_new).
/// If a user exists with the same email, the Apple ID is linked to that account.
pub async fn find_or_create_apple(
    pool:         &PgPool,
    apple_id:     &str,
    email:        Option<&str>,
    display_name: Option<&str>,
) -> AppResult<(UserRow, bool)> {
    // 1. Existing user by Apple ID (fast path — most sign-ins).
    if let Some(user) = find_by_apple_user_id(pool, apple_id).await? {
        return Ok((user, false));
    }

    // 2. Existing user by email — link the Apple ID so future sign-ins skip step 1.
    if let Some(email) = email {
        if let Some(user) = find_by_email(pool, email).await? {
            sqlx::query("UPDATE users SET apple_user_id = $1 WHERE id = $2")
                .bind(apple_id)
                .bind(user.id)
                .execute(pool)
                .await
                .map_err(crate::error::AppError::Db)?;

            return Ok((user, false));
        }
    }

    // 3. New user.
    let email = email
        .map(String::from)
        .unwrap_or_else(|| format!("{apple_id}@privaterelay.appleid.com"));

    let user_code = generate_unique_code(pool).await?;

    let user: UserRow = sqlx::query_as(&format!(
        "INSERT INTO users (apple_user_id, email, display_name, user_code)
         VALUES ($1, $2, $3, $4)
         RETURNING {USER_COLS}"
    ))
    .bind(apple_id)
    .bind(&email)
    .bind(display_name)
    .bind(&user_code)
    .fetch_one(pool)
    .await
    .map_err(crate::error::AppError::Db)?;

    Ok((user, true))
}

/// Auto-verify the user's table_verified flag if their email matches
/// a confirmed table booking. Runs fire-and-forget after sign-in.
pub async fn maybe_verify_from_booking(pool: &PgPool, user_id: i32, email: &str) {
    let _ = sqlx::query(
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
}

// ── Operator login ────────────────────────────────────────────────────────────

/// Find the shop user associated with a location's staff PIN.
pub async fn find_operator(pool: &PgPool, code: &str, location_id: i32) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT u.{USER_COLS}
         FROM locations l
         JOIN users u ON u.business_id = l.business_id AND u.is_shop = true
         WHERE l.staff_pin = $1 AND l.id = $2
         LIMIT 1"
    ))
    .bind(code)
    .bind(location_id)
    .fetch_optional(pool)
    .await
    .map_err(crate::error::AppError::Db)
}

// ── Email + password auth ─────────────────────────────────────────────────────

pub async fn create_email_user(
    pool:         &PgPool,
    email:        &str,
    password_hash: &str,
    display_name:  Option<&str>,
) -> AppResult<UserRow> {
    let user_code = generate_unique_code(pool).await?;
    sqlx::query_as(&format!(
        "INSERT INTO users (email, password_hash, display_name, user_code)
         VALUES ($1, $2, $3, $4)
         RETURNING {USER_COLS}"
    ))
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .bind(&user_code)
    .fetch_one(pool)
    .await
    .map_err(crate::error::AppError::Db)
}

pub async fn set_password(pool: &PgPool, user_id: i32, password_hash: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(crate::error::AppError::Db)?;
    Ok(())
}

// ── Profile mutations ─────────────────────────────────────────────────────────

pub async fn set_push_token(pool: &PgPool, user_id: i32, token: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET push_token = $1 WHERE id = $2")
        .bind(token)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(crate::error::AppError::Db)?;
    Ok(())
}

pub async fn set_display_name(pool: &PgPool, user_id: i32, name: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
        .bind(name)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(crate::error::AppError::Db)?;
    Ok(())
}

// ── Password reset tokens ─────────────────────────────────────────────────────

pub async fn create_reset_token(pool: &PgPool, user_id: i32, token: &str) -> AppResult<()> {
    let expires_at = chrono::Utc::now().naive_utc() + chrono::Duration::hours(1);
    // Store in a simple metadata column if available; use a dedicated table if one exists.
    // The TypeScript server stored reset tokens inline — adapt to schema reality.
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
    .map_err(crate::error::AppError::Db)?;
    Ok(())
}

pub async fn consume_reset_token(pool: &PgPool, token: &str) -> AppResult<Option<i32>> {
    let row: Option<(i32,)> = sqlx::query_as(
        "DELETE FROM password_reset_tokens
         WHERE token = $1 AND expires_at > NOW()
         RETURNING user_id",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(crate::error::AppError::Db)?;
    Ok(row.map(|(id,)| id))
}

// ── Table booking claim ───────────────────────────────────────────────────────

pub async fn claim_booking_email(pool: &PgPool, user_id: i32, email: &str) -> AppResult<bool> {
    // Link the booking email to the user and verify their table_verified flag.
    let matched: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM table_bookings
             WHERE LOWER(email) = LOWER($1) AND status = 'confirmed'
         )",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(crate::error::AppError::Db)?;

    if matched {
        sqlx::query("UPDATE users SET table_verified = true WHERE id = $1")
            .bind(user_id)
            .execute(pool)
            .await
            .map_err(crate::error::AppError::Db)?;
    }

    Ok(matched)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Retry-loop that generates a unique 6-character user code, resolving the
/// (rare) case where a randomly generated code already exists.
async fn generate_unique_code(pool: &PgPool) -> AppResult<String> {
    loop {
        let code = random_code();
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE user_code = $1)")
                .bind(&code)
                .fetch_one(pool)
                .await
                .map_err(crate::error::AppError::Db)?;
        if !exists {
            return Ok(code);
        }
    }
}

fn random_code() -> String {
    // Excludes visually ambiguous characters (0/O, 1/I).
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}
