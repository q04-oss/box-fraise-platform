#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttestationTokenRow {
    pub id:                               i32,
    pub user_id:                          i32,
    pub soultoken_id:                     i32,
    pub scope:                            String,
    pub scope_metadata:                   Option<serde_json::Value>,
    pub token_hash:                       String,
    pub requesting_business_soultoken_id: Option<i32>,
    pub user_device_id:                   Option<String>,
    /// CAST(presentation_latitude AS FLOAT8)
    pub presentation_latitude:            Option<f64>,
    /// CAST(presentation_longitude AS FLOAT8)
    pub presentation_longitude:           Option<f64>,
    pub presentation_device_signature:    Option<String>,
    pub user_presented:                   bool,
    pub issued_at:                        DateTime<Utc>,
    pub expires_at:                       DateTime<Utc>,
    pub verified_at:                      Option<DateTime<Utc>>,
    pub revoked_at:                       Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ThirdPartyVerificationAttemptRow {
    pub id:                               i32,
    pub token_hash:                       String,
    pub attestation_token_id:             Option<i32>,
    pub requesting_business_soultoken_id: Option<i32>,
    pub request_signature:                Option<String>,
    pub ip_address:                       Option<String>,
    pub user_agent:                       Option<String>,
    pub outcome:                          String,
    pub attempted_at:                     DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct IssueAttestationTokenRequest {
    pub scope:                            String,
    pub requesting_business_soultoken_id: Option<i32>,
    pub user_device_id:                   Option<String>,
    pub presentation_latitude:            Option<f64>,
    pub presentation_longitude:           Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyAttestationTokenRequest {
    pub raw_token:                        String,
    pub requesting_business_soultoken_id: Option<i32>,
    pub request_signature:                Option<String>,
}

/// Returned ONCE on issuance — raw_token is never stored or returned again.
#[derive(Debug, Serialize)]
pub struct AttestationTokenResponse {
    pub raw_token:  String,
    pub scope:      String,
    pub expires_at: DateTime<Utc>,
    pub issued_at:  DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct VerificationResultResponse {
    pub valid:       bool,
    pub scope:       Option<String>,
    pub outcome:     String,
    pub verified_at: Option<DateTime<Utc>>,
}

/// Token metadata returned for list endpoints — never includes raw_token.
#[derive(Debug, Clone, Serialize)]
pub struct AttestationTokenMeta {
    pub id:         i32,
    pub scope:      String,
    pub issued_at:  DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub const ATTESTATION_TOKEN_COLS: &str =
    "id, user_id, soultoken_id, scope, scope_metadata, token_hash, \
     requesting_business_soultoken_id, user_device_id, \
     CAST(presentation_latitude  AS FLOAT8) AS presentation_latitude, \
     CAST(presentation_longitude AS FLOAT8) AS presentation_longitude, \
     presentation_device_signature, user_presented, \
     issued_at, expires_at, verified_at, revoked_at";
