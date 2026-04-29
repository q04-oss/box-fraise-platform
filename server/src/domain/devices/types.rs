use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

// ── Stored rows ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DeviceRow {
    pub id:             i32,
    pub device_address: String,
    pub user_id:        i32,
    pub role:           String,
    pub created_at:     NaiveDateTime,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterDeviceBody {
    /// Device's Ethereum address (0x…).
    pub device_address: String,
    /// Signature over the pairing token using the device's private key.
    pub signature:      String,
    /// The 8-character pairing code from `/api/devices/pair-token`.
    pub pairing_token:  String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRoleBody {
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct AttestBody {
    /// App Attest key ID (from DCAppAttestService).
    pub key_id:      String,
    /// Raw attestation object (CBOR, base64).
    pub attestation: String,
    /// Per-device HMAC signing key (32 random bytes, base64) — generated on-device.
    pub hmac_key:    String,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct PairTokenResponse {
    pub token: String,
}
