use sqlx::PgPool;

use crate::{error::{AppError, AppResult}, types::UserId};
use super::types::*;

// ── Search ────────────────────────────────────────────────────────────────────

/// Case-insensitive display_name search. Only verified, non-banned users.
/// Capped at 8 results to limit enumeration.
pub async fn search(pool: &PgPool, query: &str) -> AppResult<Vec<UserSearchResult>> {
    sqlx::query_as::<_, UserSearchResult>(
        "SELECT id, display_name, portrait_url, verified, user_code
         FROM users
         WHERE display_name ILIKE $1
           AND verified = true
           AND banned   = false
         ORDER BY display_name
         LIMIT 8",
    )
    .bind(format!("%{}%", query.trim()))
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

// ── Public profile ────────────────────────────────────────────────────────────

pub async fn public_profile(pool: &PgPool, id: UserId) -> AppResult<Option<PublicProfile>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id:           UserId,
        display_name: Option<String>,
        portrait_url: Option<String>,
        is_dj:        bool,
        verified:     bool,
        user_code:    Option<String>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT id, display_name, portrait_url, is_dj, verified, user_code
         FROM users
         WHERE id = $1 AND banned = false",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|r| PublicProfile {
        id:           r.id,
        display_name: r.display_name,
        portrait_url: r.portrait_url,
        is_dj:        r.is_dj,
        verified:     r.verified,
        user_code:    r.user_code,
    }))
}

// ── Social access ─────────────────────────────────────────────────────────────

pub async fn social_access(pool: &PgPool, user_id: UserId) -> AppResult<Option<SocialAccess>> {
    sqlx::query_as::<_, SocialAccess>(
        "SELECT social_tier, social_time_bank_seconds
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Notifications ─────────────────────────────────────────────────────────────

pub async fn list_notifications(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Vec<NotificationRow>> {
    sqlx::query_as::<_, NotificationRow>(
        "SELECT id, user_id, type AS notif_type, title, body, read, data, created_at
         FROM notifications
         WHERE user_id = $1
         ORDER BY created_at DESC
         LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

/// Ownership enforced in WHERE — users cannot read-mark other users' notifications.
pub async fn mark_read(pool: &PgPool, user_id: UserId, notif_id: i32) -> AppResult<()> {
    let result = sqlx::query(
        "UPDATE notifications SET read = true
         WHERE id = $1 AND user_id = $2",
    )
    .bind(notif_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn mark_all_read(pool: &PgPool, user_id: UserId) -> AppResult<()> {
    sqlx::query(
        "UPDATE notifications SET read = true
         WHERE user_id = $1 AND read = false",
    )
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

