use sqlx::PgPool;

use crate::{error::{AppError, AppResult}, types::UserId};
use super::types::{MemberRow, MembershipRow};

pub async fn find_for_user(
    pool:    &PgPool,
    user_id: UserId,
) -> AppResult<Option<MembershipRow>> {
    sqlx::query_as::<_, MembershipRow>(
        "SELECT id, user_id, tier, status, started_at, renews_at
         FROM memberships
         WHERE user_id = $1
         ORDER BY started_at DESC
         LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn list_active_members(pool: &PgPool) -> AppResult<Vec<MemberRow>> {
    sqlx::query_as::<_, MemberRow>(
        "SELECT m.user_id, u.display_name, u.portrait_url, m.tier
         FROM memberships m
         JOIN users u ON u.id = m.user_id
         WHERE m.status = 'active'
           AND u.banned = false
         ORDER BY m.started_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}
