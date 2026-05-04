#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VerificationEventRow {
    pub id:             i32,
    pub user_id:        i32,
    pub event_type:     String,
    pub from_status:    Option<String>,
    pub to_status:      Option<String>,
    pub reference_id:   Option<i32>,
    pub reference_type: Option<String>,
    pub actor_id:       Option<i32>,
    pub metadata:       serde_json::Value,
    pub created_at:     DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AuditRequestLogRow {
    pub id:              i32,
    pub user_id:         i32,
    pub requested_by:    i32,
    pub delivery_method: String,
    pub requested_at:    DateTime<Utc>,
}

// ── Response types ────────────────────────────────────────────────────────────

/// A single verification journey event — internal fields (actor_id, reference_id) excluded.
#[derive(Debug, Clone, Serialize)]
pub struct VerificationEventResponse {
    pub id:             i32,
    pub event_type:     String,
    pub from_status:    Option<String>,
    pub to_status:      Option<String>,
    pub reference_type: Option<String>,
    pub metadata:       serde_json::Value,
    pub created_at:     DateTime<Utc>,
}

/// Soultoken summary — uuid never exposed.
#[derive(Debug, Clone, Serialize)]
pub struct SoultokenSummary {
    pub display_code:      String,
    pub token_type:        String,
    pub issued_at:         DateTime<Utc>,
    pub expires_at:        DateTime<Utc>,
    pub revoked_at:        Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresenceEventSummary {
    pub event_type:       String,
    pub business_id:      i32,
    pub calendar_date:    String,
    pub is_qualifying:    bool,
    pub rejection_reason: Option<String>,
    pub occurred_at:      DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttestationSummary {
    pub status:         String,
    pub attempt_number: i32,
    pub visit_id:       i32,
    pub created_at:     DateTime<Utc>,
}

/// Attestation token summary — token_hash never exposed.
#[derive(Debug, Clone, Serialize)]
pub struct AttestationTokenSummary {
    pub scope:       String,
    pub issued_at:   DateTime<Utc>,
    pub expires_at:  DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub revoked_at:  Option<DateTime<Utc>>,
}

/// Full BFIP Section 17 audit trail response.
#[derive(Debug, Serialize)]
pub struct UserAuditTrailResponse {
    pub user_id:              i32,
    pub verification_journey: Vec<VerificationEventResponse>,
    pub soultoken_history:    Vec<SoultokenSummary>,
    pub presence_history:     Vec<PresenceEventSummary>,
    pub attestation_history:  Vec<AttestationSummary>,
    pub token_history:        Vec<AttestationTokenSummary>,
    pub requested_at:         DateTime<Utc>,
}

pub const VERIFICATION_EVENT_COLS: &str =
    "id, user_id, event_type, from_status, to_status, reference_id, \
     reference_type, actor_id, metadata, created_at";
