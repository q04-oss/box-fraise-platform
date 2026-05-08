#![allow(missing_docs)]
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgConnection;

use crate::error::{AppResult, DomainError};
use super::types::{IdentityCredentialRow, IDENTITY_CREDENTIAL_COLS};

// ── Credentials ───────────────────────────────────────────────────────────────

pub async fn create_identity_credential(
    conn:                &mut PgConnection,
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
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_identity_credential_by_id(
    conn: &mut PgConnection,
    id:   i32,
) -> AppResult<Option<IdentityCredentialRow>> {
    sqlx::query_as(&format!(
        "SELECT {IDENTITY_CREDENTIAL_COLS} FROM identity_credentials WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_identity_credential_by_session(
    conn:       &mut PgConnection,
    session_id: &str,
) -> AppResult<Option<IdentityCredentialRow>> {
    sqlx::query_as(&format!(
        "SELECT {IDENTITY_CREDENTIAL_COLS} FROM identity_credentials \
         WHERE external_session_id = $1"
    ))
    .bind(session_id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_latest_credential_by_user(
    conn:    &mut PgConnection,
    user_id: i32,
) -> AppResult<Option<IdentityCredentialRow>> {
    sqlx::query_as(&format!(
        "SELECT {IDENTITY_CREDENTIAL_COLS} FROM identity_credentials \
         WHERE user_id = $1 ORDER BY created_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_stripe_webhook(
    conn:          &mut PgConnection,
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
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn complete_cooling(
    conn:          &mut PgConnection,
    credential_id: i32,
) -> AppResult<IdentityCredentialRow> {
    sqlx::query_as(&format!(
        "UPDATE identity_credentials \
         SET cooling_completed_at = now() \
         WHERE id = $1 \
         RETURNING {IDENTITY_CREDENTIAL_COLS}"
    ))
    .bind(credential_id)
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

// ── Cooling events ────────────────────────────────────────────────────────────

/// Insert a cooling-period app-open event.
/// Returns `true` if a new row was created, `false` on same-day duplicate.
pub async fn insert_cooling_event(
    conn:                 &mut PgConnection,
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
    .execute(conn)
    .await
    .map_err(DomainError::Db)?
    .rows_affected();
    Ok(rows > 0)
}

/// Count distinct calendar days with a qualifying app open for this credential.
pub async fn count_cooling_days(
    conn:          &mut PgConnection,
    user_id:       i32,
    credential_id: i32,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(DISTINCT calendar_date) FROM cooling_period_events \
         WHERE user_id = $1 AND credential_id = $2"
    )
    .bind(user_id)
    .bind(credential_id)
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

// ── App Attest key storage (Grade A item 1) ──────────────────────────────────

/// Look up the registered App Attest public key (DER SPKI) for a given key_id.
///
/// Returns `Ok(None)` when no row matches — the assertion came from a key
/// that was never registered, so verification cannot proceed. Under
/// app_user RLS, only the requesting user's own credential rows are
/// visible, which is the correct scope: a user must use their own attested
/// device.
pub async fn get_app_attest_public_key(
    conn:   &mut PgConnection,
    key_id: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT app_attest_public_key_der \
         FROM identity_credentials \
         WHERE app_attest_key_id = $1 \
           AND app_attest_public_key_der IS NOT NULL \
         LIMIT 1"
    )
    .bind(key_id)
    .fetch_optional(conn)
    .await
}

/// Persist the device public key on the caller's most recent identity
/// credential row. The `app_attest_key_id IS NULL` guard makes this a
/// one-shot registration — re-registering must explicitly clear the
/// previous key first (admin-only path).
///
/// Returns the number of rows updated: 0 means either the user has no
/// identity credential yet OR they've already registered a key on it
/// — the route handler maps 0 → `Conflict`.
pub async fn register_app_attest_key(
    conn:           &mut PgConnection,
    user_id:        i32,
    key_id:         &str,
    public_key_der: Vec<u8>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE identity_credentials \
         SET app_attest_key_id         = $1, \
             app_attest_public_key_der = $2 \
         WHERE user_id = $3 \
           AND app_attest_key_id IS NULL"
    )
    .bind(key_id)
    .bind(public_key_der)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(result.rows_affected())
}
