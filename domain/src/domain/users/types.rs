use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Public-safe user projection ───────────────────────────────────────────────

/// Public-facing user profile — contains no private or sensitive fields.
#[derive(Debug, Serialize)]
pub struct PublicProfile {
    /// User identifier.
    pub id:                  UserId,
    /// Display name chosen by the user (may be absent).
    pub display_name:        Option<String>,
    /// Whether the user's email has been verified.
    pub email_verified:      bool,
    /// BFIP verification status (registered / identity_confirmed / presence_confirmed / attested).
    pub verification_status: String,
    /// Whether the user holds a soultoken.
    pub soultoken_id:        Option<i32>,
}

/// Compact user result returned by the search endpoint.
#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct UserSearchResult {
    /// User identifier.
    pub id:                  UserId,
    /// Display name chosen by the user.
    pub display_name:        Option<String>,
    /// Whether the user's email has been verified.
    pub email_verified:      bool,
    /// BFIP verification status.
    pub verification_status: String,
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Query parameters for `GET /api/users/search`.
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    /// Free-text search term matched against display name and email.
    pub q: String,
}

// ── Compliance — Hardening §9 ────────────────────────────────────────────────

/// Response from `DELETE /api/users/me`. Records what was anonymised in
/// place and what is retained for legal evidence.
#[derive(Debug, Serialize)]
pub struct ErasureResponse {
    /// Erased user's id.
    pub user_id:              i32,
    /// When the erasure ran (server-side).
    pub erasure_scheduled_at: DateTime<Utc>,
    /// Human-readable description of every retention rule that left a
    /// trace behind.
    pub retained_data:        Vec<String>,
    /// Plain-language note for the audit trail and the user response.
    pub erasure_note:         String,
}

/// Top-level `GET /api/users/me/export` response (GDPR Article 20).
#[derive(Debug, Serialize)]
pub struct UserDataExport {
    /// When the export was generated (server-side).
    pub exported_at:          DateTime<Utc>,
    /// Identity profile.
    pub user_profile:         UserProfileExport,
    /// Verification journey (status transitions, attestations, etc.).
    pub verification_journey: Vec<VerificationEventExport>,
    /// Presence event summaries (no GPS coordinates — only timestamps + business).
    pub presence_history:     Vec<PresenceEventSummary>,
    /// Order history (one row per order).
    pub order_history:        Vec<OrderExport>,
    /// Consent grants and revocations.
    pub consent_history:      Vec<ConsentExport>,
    /// Soultoken records (issued / renewed / revoked).
    pub soultoken_history:    Vec<SoultokenSummary>,
}

/// Identity profile section of the export.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct UserProfileExport {
    /// User identifier.
    pub id:                  i32,
    /// Email on record.
    pub email:               String,
    /// Display name (may be absent).
    pub display_name:        Option<String>,
    /// BFIP verification status.
    pub verification_status: String,
    /// When the user account was created.
    pub created_at:          DateTime<Utc>,
}

/// Verification-event row in the export.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct VerificationEventExport {
    /// Event identifier.
    pub id:           i32,
    /// Event type (`identity_confirmed`, `attestation_approved`, etc.).
    pub event_type:   String,
    /// When the event occurred.
    pub created_at:   DateTime<Utc>,
}

/// Presence event summary — no GPS, just business + timestamp.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PresenceEventSummary {
    /// Event identifier.
    pub id:           i32,
    /// Business where the event was recorded.
    pub business_id:  i32,
    /// Whether the event qualified toward presence threshold.
    pub is_qualifying: bool,
    /// When the event occurred.
    pub occurred_at:  DateTime<Utc>,
}

/// Order row in the export.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderExport {
    /// Order identifier.
    pub id:           i32,
    /// Business that fulfilled / will fulfil the order.
    pub business_id:  i32,
    /// Number of boxes ordered.
    pub box_count:    i32,
    /// Order amount in cents.
    pub amount_cents: i32,
    /// Order status.
    pub status:       String,
    /// When the order was created.
    pub created_at:   DateTime<Utc>,
}

/// Consent record in the export.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ConsentExport {
    /// Consent type (`platform_terms`, `privacy_policy`, ...).
    pub consent_type:    String,
    /// Version string of the policy that was accepted.
    pub consent_version: String,
    /// Whether consent was granted (vs revoked).
    pub granted:         bool,
    /// When consent was granted.
    pub granted_at:      DateTime<Utc>,
    /// When consent was revoked (NULL if still active).
    pub revoked_at:      Option<DateTime<Utc>>,
}

/// Soultoken summary row in the export. Excludes the internal UUID.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SoultokenSummary {
    /// Soultoken identifier.
    pub id:            i32,
    /// Public-facing display code.
    pub display_code:  String,
    /// Token type — `user`, `business`, or `cleared`.
    pub token_type:    String,
    /// When the soultoken was issued.
    pub issued_at:     DateTime<Utc>,
    /// When the soultoken expires.
    pub expires_at:    DateTime<Utc>,
    /// When the soultoken was revoked (NULL if still active).
    pub revoked_at:    Option<DateTime<Utc>>,
}
