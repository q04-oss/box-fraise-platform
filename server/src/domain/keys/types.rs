use serde::{Deserialize, Serialize};

use crate::types::UserId;

// ── Stored rows ───────────────────────────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
pub struct UserKeysRow {
    pub user_id:              UserId,
    pub identity_key:         String,
    pub identity_signing_key: Option<String>,
    pub signed_pre_key:       String,
    pub signed_pre_key_sig:   String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct OtpkRow {
    pub key_id:     i32,
    pub public_key: String,
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterKeysBody {
    pub identity_key:         String,
    pub identity_signing_key: Option<String>,
    pub signed_pre_key:       String,
    pub signed_pre_key_sig:   String,
    pub one_time_pre_keys:    Vec<OneTimePreKeyItem>,
    /// Ed25519 signature over the challenge bytes, base64-encoded.
    pub challenge_sig:        Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OneTimePreKeyItem {
    pub key_id:     i32,
    pub public_key: String,
}

#[derive(Debug, Deserialize)]
pub struct UploadOtpkBody {
    pub one_time_pre_keys: Vec<OneTimePreKeyItem>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    pub challenge: String,
}

#[derive(Debug, Serialize)]
pub struct OtpkCountResponse {
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct KeyBundleResponse {
    pub user_id:              UserId,
    pub identity_key:         String,
    pub identity_signing_key: Option<String>,
    pub signed_pre_key:       String,
    pub signed_pre_key_sig:   String,
    pub one_time_pre_key:     Option<OtpkResponse>,
}

#[derive(Debug, Serialize)]
pub struct OtpkResponse {
    pub key_id:     i32,
    pub public_key: String,
}
