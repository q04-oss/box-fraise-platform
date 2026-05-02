#![allow(missing_docs)]
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdentityCredentialRow {
    pub id:                         i32,
    pub user_id:                    i32,
    pub credential_type:            String,
    pub external_session_id:        Option<String>,
    pub stripe_identity_report_id:  Option<String>,
    pub raw_verification_status:    Option<String>,
    pub response_hash:              Option<String>,
    pub cooling_app_opens_required: i32,
    pub verified_at:                DateTime<Utc>,
    pub cooling_ends_at:            DateTime<Utc>,
    pub cooling_completed_at:       Option<DateTime<Utc>>,
    pub created_at:                 DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CoolingPeriodEventRow {
    pub id:                   i32,
    pub user_id:              i32,
    pub credential_id:        i32,
    pub event_type:           String,
    pub device_identifier:    Option<String>,
    pub app_attest_assertion: Option<String>,
    pub calendar_date:        NaiveDate,
    pub occurred_at:          DateTime<Utc>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

/// All columns of `identity_credentials` for SELECT / RETURNING queries.
pub const IDENTITY_CREDENTIAL_COLS: &str =
    "id, user_id, credential_type, external_session_id, stripe_identity_report_id, \
     raw_verification_status, response_hash, cooling_app_opens_required, \
     verified_at, cooling_ends_at, cooling_completed_at, created_at";

/// All columns of `cooling_period_events` for SELECT queries.
pub const COOLING_PERIOD_EVENT_COLS: &str =
    "id, user_id, credential_id, event_type, device_identifier, \
     app_attest_assertion, calendar_date, occurred_at";

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/identity/verify`.
#[derive(Debug, Deserialize)]
pub struct InitiateVerificationRequest {
    pub stripe_session_id: String,
}

/// Request body for `POST /api/identity/cooling/app-open`.
#[derive(Debug, Deserialize)]
pub struct RecordAppOpenRequest {
    pub credential_id:        i32,
    pub device_identifier:    Option<String>,
    pub app_attest_assertion: Option<String>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Response from `POST /api/identity/verify`.
#[derive(Debug, Serialize)]
pub struct IdentityCredentialResponse {
    pub id:                         i32,
    pub credential_type:            String,
    pub external_session_id:        Option<String>,
    pub verified_at:                DateTime<Utc>,
    pub cooling_ends_at:            DateTime<Utc>,
    pub cooling_app_opens_required: i32,
    pub cooling_completed_at:       Option<DateTime<Utc>>,
    pub raw_verification_status:    Option<String>,
    pub created_at:                 DateTime<Utc>,
}

/// Response from cooling-period endpoints.
#[derive(Debug, Serialize)]
pub struct CoolingStatusResponse {
    pub credential_id:        i32,
    pub days_completed:       i64,
    pub days_required:        i32,
    pub cooling_ends_at:      DateTime<Utc>,
    pub cooling_completed_at: Option<DateTime<Utc>>,
    pub is_complete:          bool,
}
