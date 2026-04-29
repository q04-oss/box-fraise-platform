use sqlx::PgPool;

use crate::error::{AppError, AppResult};
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

pub async fn public_profile(pool: &PgPool, id: i32) -> AppResult<Option<PublicProfile>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id:             i32,
        display_name:   Option<String>,
        portrait_url:   Option<String>,
        is_dj:          bool,
        verified:       bool,
        user_code:      Option<String>,
        follower_count: Option<i64>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT u.id, u.display_name, u.portrait_url, u.is_dj, u.verified, u.user_code,
                COUNT(f.follower_id)::bigint AS follower_count
         FROM users u
         LEFT JOIN user_follows f ON f.following_id = u.id
         WHERE u.id = $1 AND u.banned = false
         GROUP BY u.id",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|r| PublicProfile {
        id:             r.id,
        display_name:   r.display_name,
        portrait_url:   r.portrait_url,
        is_dj:          r.is_dj,
        verified:       r.verified,
        user_code:      r.user_code,
        follower_count: r.follower_count.unwrap_or(0),
    }))
}

// ── Social access ─────────────────────────────────────────────────────────────

pub async fn social_access(pool: &PgPool, user_id: i32) -> AppResult<Option<SocialAccess>> {
    sqlx::query_as::<_, SocialAccess>(
        "SELECT social_tier, social_time_bank_seconds
         FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Stats ─────────────────────────────────────────────────────────────────────

pub async fn stats(pool: &PgPool, user_id: i32) -> AppResult<UserStats> {
    let evening: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM evening_tokens
         WHERE user_id_1 = $1 OR user_id_2 = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    let portrait: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM portrait_tokens WHERE owner_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    let nfc: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM nfc_connections
         WHERE user_id = $1 OR connected_user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    let tier: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT tier FROM memberships
         WHERE user_id = $1 AND status = 'active'
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(UserStats {
        evening_token_count:  evening,
        portrait_token_count: portrait,
        nfc_connection_count: nfc,
        membership_tier:      tier,
    })
}

// ── Wallet ────────────────────────────────────────────────────────────────────

pub async fn set_wallet(pool: &PgPool, user_id: i32, address: &str) -> AppResult<()> {
    sqlx::query("UPDATE users SET eth_address = $1 WHERE id = $2")
        .bind(address)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db) = e {
                if db.constraint() == Some("users_eth_address_key") {
                    return AppError::conflict(
                        "wallet address already linked to another account",
                    );
                }
            }
            AppError::Db(e)
        })?;
    Ok(())
}

// ── Follows ───────────────────────────────────────────────────────────────────

pub async fn follow(pool: &PgPool, follower_id: i32, following_id: i32) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO user_follows (follower_id, following_id)
         VALUES ($1, $2)
         ON CONFLICT DO NOTHING",
    )
    .bind(follower_id)
    .bind(following_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn unfollow(pool: &PgPool, follower_id: i32, following_id: i32) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM user_follows
         WHERE follower_id = $1 AND following_id = $2",
    )
    .bind(follower_id)
    .bind(following_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn follow_status(
    pool:         &PgPool,
    follower_id:  i32,
    following_id: i32,
) -> AppResult<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
             SELECT 1 FROM user_follows
             WHERE follower_id = $1 AND following_id = $2
         )",
    )
    .bind(follower_id)
    .bind(following_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn follower_count(pool: &PgPool, user_id: i32) -> AppResult<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM user_follows WHERE following_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn list_followers(pool: &PgPool, user_id: i32) -> AppResult<Vec<UserSearchResult>> {
    sqlx::query_as::<_, UserSearchResult>(
        "SELECT u.id, u.display_name, u.portrait_url, u.verified, u.user_code
         FROM user_follows f
         JOIN users u ON u.id = f.follower_id
         WHERE f.following_id = $1 AND u.banned = false
         ORDER BY f.created_at DESC
         LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn list_following(pool: &PgPool, user_id: i32) -> AppResult<Vec<UserSearchResult>> {
    sqlx::query_as::<_, UserSearchResult>(
        "SELECT u.id, u.display_name, u.portrait_url, u.verified, u.user_code
         FROM user_follows f
         JOIN users u ON u.id = f.following_id
         WHERE f.follower_id = $1 AND u.banned = false
         ORDER BY f.created_at DESC
         LIMIT 100",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

// ── Notifications ─────────────────────────────────────────────────────────────

pub async fn list_notifications(
    pool:    &PgPool,
    user_id: i32,
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
pub async fn mark_read(pool: &PgPool, user_id: i32, notif_id: i32) -> AppResult<()> {
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

pub async fn mark_all_read(pool: &PgPool, user_id: i32) -> AppResult<()> {
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

// ── Feed ──────────────────────────────────────────────────────────────────────

pub async fn feed(pool: &PgPool, user_id: i32) -> AppResult<Vec<FeedItem>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        user_id:      i32,
        display_name: Option<String>,
        portrait_url: Option<String>,
        variety_name: String,
        location_name: String,
        created_at:   chrono::NaiveDateTime,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT u.id AS user_id, u.display_name, u.portrait_url,
                v.name AS variety_name, l.name AS location_name,
                o.created_at
         FROM orders o
         JOIN users     u ON u.id = o.user_id
         JOIN varieties v ON v.id = o.variety_id
         JOIN locations l ON l.id = o.location_id
         WHERE o.user_id IN (
             SELECT following_id FROM user_follows WHERE follower_id = $1
         )
         AND o.status = 'collected'
         ORDER BY o.created_at DESC
         LIMIT 30",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(rows
        .into_iter()
        .map(|r| FeedItem {
            user_id:      r.user_id,
            display_name: r.display_name,
            portrait_url: r.portrait_url,
            event:        "collected_order".to_owned(),
            data: serde_json::json!({
                "variety_name":  r.variety_name,
                "location_name": r.location_name,
            }),
            created_at: r.created_at,
        })
        .collect())
}
