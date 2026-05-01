use rand::Rng;
use sqlx::PgPool;

use crate::{error::{DomainError, AppResult}, types::UserId};
use super::types::{UserRow, USER_COLS};

// ── Lookups ───────────────────────────────────────────────────────────────────

pub async fn find_by_id(pool: &PgPool, id: UserId) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!("SELECT {USER_COLS} FROM users WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(DomainError::Db)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE LOWER(email) = LOWER($1)"
    ))
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
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
    let mut tx = pool.begin().await.map_err(DomainError::Db)?;

    // 1. Look up by Apple user ID first — fastest path for returning users.
    let existing: Option<UserRow> = sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE apple_user_id = $1 FOR UPDATE"
    ))
    .bind(apple_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(DomainError::Db)?;

    if let Some(user) = existing {
        tx.commit().await.map_err(DomainError::Db)?;
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
    .map_err(DomainError::Db)?;

    tx.commit().await.map_err(DomainError::Db)?;

    let is_new = email.is_none() || email_str.ends_with("@privaterelay.appleid.com");

    Ok((user, is_new))
}

pub async fn find_or_create_magic_link_user(
    pool:  &PgPool,
    email: &str,
) -> AppResult<(UserRow, bool)> {
    if let Some(user) = find_by_email(pool, email).await? {
        return Ok((user, false));
    }
    let user_code = generate_unique_code(pool).await?;
    let row: Option<UserRow> = sqlx::query_as(&format!(
        "INSERT INTO users (email, user_code, verified)
         VALUES ($1, $2, true)
         ON CONFLICT (email) DO NOTHING
         RETURNING {USER_COLS}"
    ))
    .bind(email)
    .bind(&user_code)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;

    match row {
        Some(user) => Ok((user, true)),
        None => {
            let user = find_by_email(pool, email).await?
                .ok_or_else(|| DomainError::Internal(anyhow::anyhow!(
                    "magic link user creation race"
                )))?;
            Ok((user, false))
        }
    }
}

// ── Profile mutations ─────────────────────────────────────────────────────────

pub async fn set_push_token(pool: &PgPool, user_id: UserId, token: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET push_token = $1 WHERE id = $2")
        .bind(token)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    Ok(())
}

pub async fn set_display_name(pool: &PgPool, user_id: UserId, name: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET display_name = $1 WHERE id = $2")
        .bind(name)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    Ok(())
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
                .map_err(DomainError::Db)?;
        if !exists {
            return Ok(code);
        }
    }
    Err(DomainError::Internal(anyhow::anyhow!(
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
                .map_err(DomainError::Db)?;
        if !exists {
            return Ok(code);
        }
    }
    Err(DomainError::Internal(anyhow::anyhow!(
        "could not generate a unique user_code after 10 attempts"
    )))
}

pub async fn set_verified(pool: &PgPool, user_id: UserId) -> AppResult<()> {
    sqlx::query("UPDATE users SET verified = true WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    Ok(())
}

/// Excludes visually ambiguous characters (0/O, 1/I/L).
fn random_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}
