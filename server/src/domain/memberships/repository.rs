use sqlx::PgPool;

use crate::error::{AppError, AppResult};
use super::types::{MemberRow, MembershipRow};

pub async fn find_for_user(
    pool:    &PgPool,
    user_id: i32,
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

pub async fn join_waitlist(
    pool:    &PgPool,
    user_id: i32,
    tier:    &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO membership_waitlist (user_id, tier)
         VALUES ($1, $2)
         ON CONFLICT (user_id, tier) DO NOTHING",
    )
    .bind(user_id)
    .bind(tier)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn list_fund_contributors(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<serde_json::Value>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        amount_cents:  i32,
        display_name:  Option<String>,
        portrait_url:  Option<String>,
        created_at:    chrono::NaiveDateTime,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT fc.amount_cents, u.display_name, u.portrait_url, fc.created_at
         FROM fund_contributions fc
         JOIN users u ON u.id = fc.contributor_id
         WHERE fc.recipient_id = $1
         ORDER BY fc.created_at DESC
         LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(rows
        .into_iter()
        .map(|r| serde_json::json!({
            "amount_cents":  r.amount_cents,
            "display_name":  r.display_name,
            "portrait_url":  r.portrait_url,
            "created_at":    r.created_at,
        }))
        .collect())
}
