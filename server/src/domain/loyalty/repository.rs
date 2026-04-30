use sqlx::PgPool;

use crate::{error::{AppError, AppResult}, types::UserId};
use super::types::{LoyaltyConfig, LoyaltyEventRow};

// ── Config ────────────────────────────────────────────────────────────────────

pub async fn get_config(pool: &PgPool, business_id: i32) -> AppResult<Option<LoyaltyConfig>> {
    sqlx::query_as(
        "SELECT steeps_per_reward, reward_description
         FROM business_loyalty_config
         WHERE business_id = $1"
    )
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Balance ───────────────────────────────────────────────────────────────────

pub struct RawBalance {
    pub steeps_earned:    i64,
    pub rewards_redeemed: i64,
}

pub async fn get_balance(
    pool:        &PgPool,
    user_id:     UserId,
    business_id: i32,
) -> AppResult<RawBalance> {
    let row: (i64, i64) = sqlx::query_as(
        "SELECT
             COUNT(*) FILTER (WHERE event_type = 'steep_earned')    AS steeps_earned,
             COUNT(*) FILTER (WHERE event_type = 'reward_redeemed') AS rewards_redeemed
         FROM loyalty_events
         WHERE user_id = $1 AND business_id = $2"
    )
    .bind(user_id)
    .bind(business_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(RawBalance { steeps_earned: row.0, rewards_redeemed: row.1 })
}

// ── History ───────────────────────────────────────────────────────────────────

pub async fn get_history(
    pool:        &PgPool,
    user_id:     UserId,
    business_id: i32,
    limit:       i64,
    offset:      i64,
) -> AppResult<Vec<LoyaltyEventRow>> {
    sqlx::query_as(
        "SELECT id, event_type, source, metadata, created_at
         FROM loyalty_events
         WHERE user_id = $1 AND business_id = $2
         ORDER BY created_at DESC
         LIMIT $3 OFFSET $4"
    )
    .bind(user_id)
    .bind(business_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

// ── Event insert ──────────────────────────────────────────────────────────────

pub async fn insert_event(
    pool:            &PgPool,
    user_id:         UserId,
    business_id:     i32,
    event_type:      &str,
    source:          &str,
    idempotency_key: &str,
    metadata:        serde_json::Value,
) -> AppResult<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO loyalty_events
             (user_id, business_id, event_type, source, idempotency_key, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id"
    )
    .bind(user_id)
    .bind(business_id)
    .bind(event_type)
    .bind(source)
    .bind(idempotency_key)
    .bind(metadata)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(id)
}

// ── Customer lookup (for stamp page) ─────────────────────────────────────────

pub async fn get_customer_name(pool: &PgPool, user_id: UserId) -> AppResult<String> {
    let (name,): (Option<String>,) = sqlx::query_as(
        "SELECT display_name FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(name.unwrap_or_else(|| "Guest".to_string()))
}

// ── NFC stickers ──────────────────────────────────────────────────────────────

/// Registers a sticker to a business on first activation. Returns the
/// business_id that owns this sticker (which may be an existing registration).
/// If the sticker was previously registered to a different business, returns Err.
pub async fn upsert_nfc_sticker(
    pool:        &PgPool,
    uuid:        &str,
    business_id: i32,
) -> AppResult<i32> {
    // Try to insert; if it already exists for this business, that's fine.
    let result = sqlx::query(
        "INSERT INTO nfc_stickers (uuid, business_id)
         VALUES ($1, $2)
         ON CONFLICT (uuid) DO NOTHING"
    )
    .bind(uuid)
    .bind(business_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    // If nothing was inserted, the sticker exists — fetch its business_id.
    if result.rows_affected() == 0 {
        let (owner,): (i32,) = sqlx::query_as(
            "SELECT business_id FROM nfc_stickers WHERE uuid = $1"
        )
        .bind(uuid)
        .fetch_one(pool)
        .await
        .map_err(AppError::Db)?;

        if owner != business_id {
            return Err(AppError::Forbidden); // sticker belongs to a different business
        }
        return Ok(owner);
    }

    Ok(business_id)
}

pub async fn increment_nfc_taps(pool: &PgPool, uuid: &str) -> AppResult<()> {
    sqlx::query("UPDATE nfc_stickers SET total_taps = total_taps + 1 WHERE uuid = $1")
        .bind(uuid)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}
