#![allow(missing_docs)]
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{IdentityCredentialRow, IDENTITY_CREDENTIAL_COLS};

// ── Credentials ───────────────────────────────────────────────────────────────

pub async fn create_identity_credential(
    pool:                &PgPool,
    user_id:             i32,
    credential_type:     &str,
    external_session_id: Option<&str>,
    verified_at:         DateTime<Utc>,
    cooling_ends_at:     DateTime<Utc>,
) -> AppResult<IdentityCredentialRow> {
    sqlx::query_as(&format!(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, external_session_id, verified_at, cooling_ends_at) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING {IDENTITY_CREDENTIAL_COLS}"
    ))
    .bind(user_id)
    .bind(credential_type)
    .bind(external_session_id)
    .bind(verified_at)
    .bind(cooling_ends_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_identity_credential_by_id(
    pool: &PgPool,
    id:   i32,
) -> AppResult<Option<IdentityCredentialRow>> {
    sqlx::query_as(&format!(
        "SELECT {IDENTITY_CREDENTIAL_COLS} FROM identity_credentials WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_identity_credential_by_session(
    pool:       &PgPool,
    session_id: &str,
) -> AppResult<Option<IdentityCredentialRow>> {
    sqlx::query_as(&format!(
        "SELECT {IDENTITY_CREDENTIAL_COLS} FROM identity_credentials \
         WHERE external_session_id = $1"
    ))
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_latest_credential_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Option<IdentityCredentialRow>> {
    sqlx::query_as(&format!(
        "SELECT {IDENTITY_CREDENTIAL_COLS} FROM identity_credentials \
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_stripe_webhook(
    pool:          &PgPool,
    id:            i32,
    report_id:     Option<&str>,
    raw_status:    Option<&str>,
    response_hash: Option<&str>,
) -> AppResult<IdentityCredentialRow> {
    sqlx::query_as(&format!(
        "UPDATE identity_credentials \
         SET stripe_identity_report_id = COALESCE($2, stripe_identity_report_id), \
             raw_verification_status   = COALESCE($3, raw_verification_status), \
             response_hash             = COALESCE($4, response_hash) \
         WHERE id = $1 \
         RETURNING {IDENTITY_CREDENTIAL_COLS}"
    ))
    .bind(id)
    .bind(report_id)
    .bind(raw_status)
    .bind(response_hash)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn complete_cooling(
    pool:          &PgPool,
    credential_id: i32,
) -> AppResult<IdentityCredentialRow> {
    sqlx::query_as(&format!(
        "UPDATE identity_credentials \
         SET cooling_completed_at = now() \
         WHERE id = $1 \
         RETURNING {IDENTITY_CREDENTIAL_COLS}"
    ))
    .bind(credential_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Cooling events ────────────────────────────────────────────────────────────

/// Insert a cooling-period app-open event.
/// Returns `true` if a new row was created, `false` on same-day duplicate.
pub async fn insert_cooling_event(
    pool:                 &PgPool,
    user_id:              i32,
    credential_id:        i32,
    device_identifier:    Option<&str>,
    app_attest_assertion: Option<&str>,
    calendar_date:        NaiveDate,
) -> AppResult<bool> {
    let rows = sqlx::query(
        "INSERT INTO cooling_period_events \
         (user_id, credential_id, device_identifier, app_attest_assertion, calendar_date) \
         VALUES ($1, $2, $3, $4, $5) \
         ON CONFLICT (user_id, credential_id, calendar_date) DO NOTHING"
    )
    .bind(user_id)
    .bind(credential_id)
    .bind(device_identifier)
    .bind(app_attest_assertion)
    .bind(calendar_date)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?
    .rows_affected();
    Ok(rows > 0)
}

/// Count distinct calendar days with a qualifying app open for this credential.
pub async fn count_cooling_days(
    pool:          &PgPool,
    user_id:       i32,
    credential_id: i32,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(DISTINCT calendar_date) FROM cooling_period_events \
         WHERE user_id = $1 AND credential_id = $2"
    )
    .bind(user_id)
    .bind(credential_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}
