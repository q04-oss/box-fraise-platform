use serde::{Deserialize, Serialize};

use crate::types::{KeyId, UserId};

// ── Stored rows ───────────────────────────────────────────────────────────────

/// X3DH public key material stored in the `user_keys` table.
#[derive(Debug, sqlx::FromRow)]
pub struct UserKeysRow {
    /// Owner of these keys.
    pub user_id:              UserId,
    /// X25519 identity public key (base64).
    pub identity_key:         String,
    /// Ed25519 identity signing key (base64). Required for new clients.
    pub identity_signing_key: Option<String>,
    /// X25519 signed pre-key (base64).
    pub signed_pre_key:       String,
    /// Signature over the signed pre-key, verifiable with the identity signing key.
    pub signed_pre_key_sig:   String,
    /// When these keys were last updated.
    pub updated_at:           chrono::NaiveDateTime,
}

/// A single one-time pre-key row from `one_time_pre_keys`.
#[derive(Debug, sqlx::FromRow)]
pub struct OtpkRow {
    /// Identifier for this pre-key, as chosen by the key owner.
    pub key_id:     KeyId,
    /// X25519 one-time pre-key public value (base64).
    pub public_key: String,
}

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/keys/register`.
#[derive(Debug, Deserialize)]
pub struct RegisterKeysBody {
    /// X25519 identity public key (base64).
    pub identity_key:         String,
    /// Ed25519 identity signing key (base64). Required for new client versions.
    pub identity_signing_key: Option<String>,
    /// X25519 signed pre-key (base64).
    pub signed_pre_key:       String,
    /// Signature over the signed pre-key (base64).
    pub signed_pre_key_sig:   String,
    /// Batch of one-time pre-keys to upload.
    pub one_time_pre_keys:    Vec<OneTimePreKeyItem>,
    /// Ed25519 signature over the challenge bytes, base64-encoded.
    pub challenge_sig:        Option<String>,
}

/// A single one-time pre-key item in a registration or upload batch.
#[derive(Debug, Deserialize)]
pub struct OneTimePreKeyItem {
    /// Client-assigned key identifier.
    pub key_id:     KeyId,
    /// X25519 one-time pre-key public value (base64).
    pub public_key: String,
}

/// Request body for `POST /api/keys/one-time`.
#[derive(Debug, Deserialize)]
pub struct UploadOtpkBody {
    /// Batch of new one-time pre-keys to upload.
    pub one_time_pre_keys: Vec<OneTimePreKeyItem>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Response body for `POST /api/keys/challenge`.
#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    /// Random challenge string the client must sign with its identity key.
    pub challenge: String,
}

/// Response body for `GET /api/keys/one-time/count`.
#[derive(Debug, Serialize)]
pub struct OtpkCountResponse {
    /// Number of unused one-time pre-keys remaining for the authenticated user.
    pub count: i64,
}

/// Full X3DH key bundle for a target user (response body for bundle endpoints).
#[derive(Debug, Serialize)]
pub struct KeyBundleResponse {
    /// Target user's identifier.
    pub user_id:              UserId,
    /// X25519 identity public key (base64).
    pub identity_key:         String,
    /// Ed25519 identity signing key (base64).
    pub identity_signing_key: Option<String>,
    /// X25519 signed pre-key (base64).
    pub signed_pre_key:       String,
    /// Signature over the signed pre-key (base64).
    pub signed_pre_key_sig:   String,
    /// One-time pre-key consumed for this session, if any remain.
    pub one_time_pre_key:     Option<OtpkResponse>,
}

/// A single one-time pre-key returned as part of a key bundle.
#[derive(Debug, Serialize)]
pub struct OtpkResponse {
    /// Server-assigned identifier matching the one sent during registration.
    pub key_id:     KeyId,
    /// X25519 one-time pre-key public value (base64).
    pub public_key: String,
}
