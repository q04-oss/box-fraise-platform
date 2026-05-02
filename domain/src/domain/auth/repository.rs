#![allow(missing_docs)] // Repository layer — internal implementation, documented at service layer.
use sqlx::PgPool;

use crate::{error::{DomainError, AppResult}, types::UserId};
use super::types::{UserRow, USER_COLS};

// ── Lookups ───────────────────────────────────────────────────────────────────

pub async fn find_by_id(pool: &PgPool, id: UserId) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> AppResult<Option<UserRow>> {
    sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users \
         WHERE LOWER(email) = LOWER($1) AND deleted_at IS NULL"
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

    // 1. Look up by Apple ID first — fastest path for returning users.
    let existing: Option<UserRow> = sqlx::query_as(&format!(
        "SELECT {USER_COLS} FROM users \
         WHERE apple_id = $1 AND deleted_at IS NULL FOR UPDATE"
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

    let user: UserRow = sqlx::query_as(&format!(
        "INSERT INTO users (apple_id, email, display_name)
         VALUES ($1, $2, $3)
         ON CONFLICT (email) DO UPDATE
         SET apple_id = EXCLUDED.apple_id
         RETURNING {USER_COLS}"
    ))
    .bind(apple_id)
    .bind(&email_str)
    .bind(display_name)
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
    let row: Option<UserRow> = sqlx::query_as(&format!(
        "INSERT INTO users (email, email_verified)
         VALUES ($1, true)
         ON CONFLICT (email) DO NOTHING
         RETURNING {USER_COLS}"
    ))
    .bind(email)
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

pub async fn set_verified(pool: &PgPool, user_id: UserId) -> AppResult<()> {
    sqlx::query("UPDATE users SET email_verified = true WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    Ok(())
}
