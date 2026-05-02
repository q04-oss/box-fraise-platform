#![allow(missing_docs)] // Repository layer — internal implementation, documented at service layer.
use sqlx::PgPool;

use crate::{error::{DomainError, AppResult}, types::UserId};
use super::types::*;

/// Case-insensitive display_name search. Only email-verified, non-banned, non-deleted users.
/// Capped at 8 results to limit enumeration.
pub async fn search(pool: &PgPool, query: &str) -> AppResult<Vec<UserSearchResult>> {
    sqlx::query_as::<_, UserSearchResult>(
        "SELECT id, display_name, email_verified, verification_status
         FROM users
         WHERE display_name ILIKE $1
           AND email_verified = true
           AND is_banned = false
           AND deleted_at IS NULL
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
        id:                  UserId,
        display_name:        Option<String>,
        email_verified:      bool,
        verification_status: String,
        soultoken_id:        Option<i32>,
    }

    let row: Option<Row> = sqlx::query_as(
        "SELECT id, display_name, email_verified, verification_status, soultoken_id
         FROM users
         WHERE id = $1 AND is_banned = false AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)?;

    Ok(row.map(|r| PublicProfile {
        id:                  r.id,
        display_name:        r.display_name,
        email_verified:      r.email_verified,
        verification_status: r.verification_status,
        soultoken_id:        r.soultoken_id,
    }))
}
