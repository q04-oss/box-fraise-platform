#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct SoultokenRow {
    pub id:                       i32,
    pub display_code:             String,
    pub display_code_key_version: i32,
    pub schema_version:           i32,
    pub holder_user_id:           i32,
    pub token_type:               String,
    pub business_id:              Option<i32>,
    pub identity_credential_id:   Option<i32>,
    pub presence_threshold_id:    Option<i32>,
    pub attestation_id:           Option<i32>,
    pub attested_soultoken_id:    Option<i32>,
    pub vc_credential_json:       Option<serde_json::Value>,
    pub signature:                Option<String>,
    pub issued_at:                DateTime<Utc>,
    pub expires_at:               DateTime<Utc>,
    pub last_renewed_at:          Option<DateTime<Utc>>,
    pub revoked_at:               Option<DateTime<Utc>>,
    pub revocation_reason:        Option<String>,
    pub revocation_staff_id:      Option<i32>,
    pub revocation_visit_id:      Option<i32>,
    pub surrender_witnessed_by:   Option<i32>,
    pub created_at:               DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct SoultokenRenewalRow {
    pub id:                    i32,
    pub soultoken_id:          i32,
    pub user_id:               i32,
    pub triggering_presence_id: Option<i32>,
    pub renewal_type:          String,
    pub previous_expires_at:   DateTime<Utc>,
    pub new_expires_at:        DateTime<Utc>,
    pub renewed_at:            DateTime<Utc>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

/// All soultoken columns except `uuid` — never expose the internal UUID.
/// `uuid` is only used in signing operations where it is already in memory.
pub const SOULTOKEN_COLS: &str =
    "id, display_code, display_code_key_version, schema_version, holder_user_id, \
     token_type, business_id, identity_credential_id, presence_threshold_id, \
     attestation_id, attested_soultoken_id, vc_credential_json, signature, \
     issued_at, expires_at, last_renewed_at, revoked_at, revocation_reason, \
     revocation_staff_id, revocation_visit_id, surrender_witnessed_by, created_at";

pub const SOULTOKEN_RENEWAL_COLS: &str =
    "id, soultoken_id, user_id, triggering_presence_id, renewal_type, \
     previous_expires_at, new_expires_at, renewed_at";

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct IssueSoultokenRequest {
    pub attestation_id: i32,
    /// `user` | `business` | `cleared`
    pub token_type:     String,
}

#[derive(Debug, Deserialize)]
pub struct RevokeSoultokenRequest {
    pub revocation_reason:   String,
    pub revocation_visit_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SurrenderSoultokenRequest {
    pub revocation_visit_id:   i32,
    pub surrender_witnessed_by: i32,
}

#[derive(Debug, Deserialize)]
pub struct RenewSoultokenRequest {
    /// Optional FK to presence_events — None when renewing without a qualifying event reference.
    pub presence_event_id: Option<i32>,
    pub renewal_type:      String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Public soultoken response — never includes `uuid`.
#[derive(Debug, Serialize)]
pub struct SoultokenResponse {
    pub id:              i32,
    pub display_code:    String,
    pub token_type:      String,
    pub holder_user_id:  i32,
    pub issued_at:       DateTime<Utc>,
    pub expires_at:      DateTime<Utc>,
    pub last_renewed_at: Option<DateTime<Utc>>,
    pub revoked_at:      Option<DateTime<Utc>>,
    pub is_expired:      bool,
    pub is_active:       bool,
}

#[derive(Debug, Serialize)]
pub struct SoultokenRenewalResponse {
    pub soultoken_id:      i32,
    pub renewal_type:      String,
    pub previous_expires_at: DateTime<Utc>,
    pub new_expires_at:    DateTime<Utc>,
    pub renewed_at:        DateTime<Utc>,
}
