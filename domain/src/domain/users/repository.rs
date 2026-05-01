#![allow(missing_docs)] // Repository layer — internal implementation, documented at service layer.
use sqlx::PgPool;

use crate::{error::{DomainError, AppResult}, types::UserId};
use super::types::*;

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
    .map_err(DomainError::Db)
}

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
    .map_err(DomainError::Db)?;

    Ok(row.map(|r| PublicProfile {
        id:           r.id,
        display_name: r.display_name,
        portrait_url: r.portrait_url,
        is_dj:        r.is_dj,
        verified:     r.verified,
        user_code:    r.user_code,
    }))
}
