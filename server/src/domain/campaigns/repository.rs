use sqlx::PgPool;

use crate::{error::{AppError, AppResult}, types::UserId};
use super::types::{CampaignRow, SignupRow};

pub async fn list_upcoming(pool: &PgPool) -> AppResult<Vec<CampaignRow>> {
    sqlx::query_as::<_, CampaignRow>(
        "SELECT id, title, concept, salon_id, date, spots, status, created_at
         FROM campaigns
         WHERE status IN ('upcoming','open','waitlist')
         ORDER BY date ASC NULLS LAST",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn find(pool: &PgPool, id: i32) -> AppResult<Option<CampaignRow>> {
    sqlx::query_as::<_, CampaignRow>(
        "SELECT id, title, concept, salon_id, date, spots, status, created_at
         FROM campaigns WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn signup(
    pool:        &PgPool,
    user_id:     UserId,
    campaign_id: i32,
) -> AppResult<SignupRow> {
    // Determine if the campaign is full (waitlist) atomically.
    let campaign = find(pool, campaign_id).await?.ok_or(AppError::NotFound)?;

    if !matches!(campaign.status.as_str(), "open" | "waitlist") {
        return Err(AppError::bad_request("campaign is not accepting signups"));
    }

    let signup_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM campaign_signups
         WHERE campaign_id = $1 AND waitlist = false",
    )
    .bind(campaign_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    let on_waitlist = campaign.spots.map_or(false, |s| signup_count >= s as i64);

    sqlx::query_as::<_, SignupRow>(
        "INSERT INTO campaign_signups (user_id, campaign_id, waitlist)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, campaign_id) DO UPDATE
         SET waitlist = EXCLUDED.waitlist
         RETURNING id, user_id, campaign_id, waitlist, created_at",
    )
    .bind(user_id)
    .bind(campaign_id)
    .bind(on_waitlist)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn cancel(pool: &PgPool, user_id: UserId, campaign_id: i32) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM campaign_signups
         WHERE user_id = $1 AND campaign_id = $2",
    )
    .bind(user_id)
    .bind(campaign_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}
