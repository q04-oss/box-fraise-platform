use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppError, AppResult};

// ── Token storage ─────────────────────────────────────────────────────────────

pub struct EncryptedTokenRow {
    pub encrypted_access_token:  String,
    pub encrypted_refresh_token: String,
    pub merchant_id:             String,
    pub square_location_id:      String,
    pub expires_at:              DateTime<Utc>,
}

/// Upserts the OAuth token row for a business.
/// ON CONFLICT means re-running the OAuth flow for the same business safely
/// overwrites the previous token — no orphaned rows.
pub async fn upsert_tokens(
    pool:       &PgPool,
    business_id: i32,
    row:        &EncryptedTokenRow,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO square_oauth_tokens
            (business_id, encrypted_access_token, encrypted_refresh_token,
             merchant_id, square_location_id, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (business_id) DO UPDATE SET
             encrypted_access_token  = EXCLUDED.encrypted_access_token,
             encrypted_refresh_token = EXCLUDED.encrypted_refresh_token,
             merchant_id             = EXCLUDED.merchant_id,
             square_location_id      = EXCLUDED.square_location_id,
             expires_at              = EXCLUDED.expires_at,
             refreshed_at            = now()"
    )
    .bind(business_id)
    .bind(&row.encrypted_access_token)
    .bind(&row.encrypted_refresh_token)
    .bind(&row.merchant_id)
    .bind(&row.square_location_id)
    .bind(row.expires_at)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(())
}

/// Loads the encrypted token row for a business.
/// Returns None if the business has not yet connected Square.
pub async fn load_tokens(
    pool:        &PgPool,
    business_id: i32,
) -> AppResult<Option<EncryptedTokenRow>> {
    let row: Option<(String, String, String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT encrypted_access_token, encrypted_refresh_token,
                merchant_id, square_location_id, expires_at
         FROM square_oauth_tokens
         WHERE business_id = $1"
    )
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|(eat, ert, mid, slid, exp)| EncryptedTokenRow {
        encrypted_access_token:  eat,
        encrypted_refresh_token: ert,
        merchant_id:             mid,
        square_location_id:      slid,
        expires_at:              exp,
    }))
}
