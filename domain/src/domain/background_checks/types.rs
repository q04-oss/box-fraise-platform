#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BackgroundCheckRow {
    pub id:                     i32,
    pub user_id:                i32,
    pub identity_credential_id: i32,
    pub provider:               String,
    pub check_type:             String,
    pub external_check_id:      Option<String>,
    pub status:                 String,
    pub response_hash:          Option<String>,
    pub checked_at:             Option<DateTime<Utc>>,
    pub expires_at:             Option<DateTime<Utc>>,
    pub created_at:             DateTime<Utc>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

/// All columns of `background_checks` for SELECT / RETURNING queries.
pub const BACKGROUND_CHECK_COLS: &str =
    "id, user_id, identity_credential_id, provider, check_type, \
     external_check_id, status, response_hash, checked_at, expires_at, created_at";

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/background-checks/initiate`.
#[derive(Debug, Deserialize)]
pub struct InitiateCheckRequest {
    /// One of: sanctions, identity_fraud, criminal
    pub check_type: String,
    /// One of: comply_advantage, refinitiv, lexisnexis, socure
    pub provider:   String,
}

/// Incoming webhook payload from background check providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckWebhookPayload {
    pub external_check_id: String,
    /// One of: passed, failed, review_required
    pub status:            String,
    pub provider:          String,
    pub raw_response:      serde_json::Value,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Individual check result returned from the API.
#[derive(Debug, Serialize)]
pub struct BackgroundCheckResponse {
    pub id:         i32,
    pub check_type: String,
    pub provider:   String,
    pub status:     String,
    pub checked_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub is_expired: bool,
}

/// Aggregate status across all check types for a user.
#[derive(Debug, Serialize)]
pub struct BackgroundCheckStatusResponse {
    pub user_id:               i32,
    pub sanctions_status:      Option<String>,
    pub identity_fraud_status: Option<String>,
    pub criminal_status:       Option<String>,
    /// true when sanctions AND identity_fraud both have a non-expired passed result.
    pub all_required_passed:   bool,
    /// true when all_required_passed AND criminal also has a non-expired passed result.
    pub cleared_eligible:      bool,
    pub checks:                Vec<BackgroundCheckResponse>,
}
