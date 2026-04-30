/// Staff JWT — structurally and cryptographically distinct from user JWTs.
///
/// # Why a separate token type?
///
/// The user `Claims` struct carries only `user_id`. A `StaffClaims` token
/// carries both `user_id` AND `business_id` as a required (non-optional) field.
/// When `jsonwebtoken::decode::<StaffClaims>()` is called on a user JWT, serde
/// fails to deserialise the missing `business_id` field — the token is rejected
/// at the type level, not just by a runtime check.
///
/// In addition, staff tokens are signed with `STAFF_JWT_SECRET`, which is a
/// different key from `JWT_SECRET`. A compromised user token cannot be used at
/// a staff endpoint even if a deserialization bug appeared — the signature
/// check fails first.
///
/// Staff tokens have an 8-hour TTL (one shift). They are not renewable; staff
/// re-authenticate at the start of each shift.
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{config::Config, error::AppError, types::UserId};

// ── Claims ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaffClaims {
    pub user_id:     UserId,
    /// The business this staff member is authorised to act on behalf of.
    /// Non-optional — a user JWT (which has no business_id) cannot decode
    /// into this struct.
    pub business_id: i32,
    pub exp:         usize,
    pub jti:         String,
}

// ── Token operations ──────────────────────────────────────────────────────────

const SHIFT_HOURS: i64 = 8;

pub fn sign_staff_token(
    user_id:     UserId,
    business_id: i32,
    cfg:         &Config,
) -> Result<String, AppError> {
    let exp = Utc::now()
        .checked_add_signed(chrono::Duration::hours(SHIFT_HOURS))
        .unwrap()
        .timestamp() as usize;

    let claims = StaffClaims {
        user_id,
        business_id,
        exp,
        jti: Uuid::new_v4().to_string(),
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(cfg.staff_jwt_secret.expose_secret().as_bytes()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("staff token encoding failed: {e}")))
}

pub fn verify_staff_token(token: &str, cfg: &Config) -> Option<StaffClaims> {
    decode::<StaffClaims>(
        token,
        &DecodingKey::from_secret(cfg.staff_jwt_secret.expose_secret().as_bytes()),
        &Validation::default(),
    )
    .ok()
    .map(|d| d.claims)
}
