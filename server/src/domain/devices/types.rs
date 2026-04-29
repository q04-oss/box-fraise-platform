use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Stored rows ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DeviceRow {
    pub id:             i32,
    pub device_address: String,
    pub user_id:        UserId,
    pub role:           String,
    pub created_at:     NaiveDateTime,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceBody {
    pub device_address: String,
    pub signature:      String,
    pub pairing_token:  String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleBody {
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct AttestBody {
    pub key_id:      String,
    pub attestation: String,
    /// Per-device HMAC signing key (32 random bytes, base64).
    /// Sensitive — never logged or included in error responses.
    #[serde(rename = "hmac_key")]
    pub hmac_key:    String,
    /// Server-issued challenge (base64). Obtained from GET /api/devices/attest-challenge.
    /// Optional for backwards compatibility with clients pre-dating challenge enforcement.
    pub challenge:   Option<String>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PairTokenResponse {
    pub token: String,
}
