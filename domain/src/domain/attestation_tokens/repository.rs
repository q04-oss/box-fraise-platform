#![allow(missing_docs)]
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{
    AttestationTokenRow, ThirdPartyVerificationAttemptRow, ATTESTATION_TOKEN_COLS,
};

// ── Attestation tokens ────────────────────────────────────────────────────────

pub async fn create_attestation_token(
    pool:                            &PgPool,
    user_id:                         i32,
    soultoken_id:                    i32,
    scope:                           &str,
    token_hash:                      &str,
    requesting_business_soultoken_id: Option<i32>,
    user_device_id:                   Option<&str>,
    presentation_latitude:            Option<f64>,
    presentation_longitude:           Option<f64>,
    expires_at:                       chrono::DateTime<chrono::Utc>,
) -> AppResult<AttestationTokenRow> {
    sqlx::query_as(&format!(
        "INSERT INTO attestation_tokens \
         (user_id, soultoken_id, scope, token_hash, \
          requesting_business_soultoken_id, user_device_id, \
          presentation_latitude, presentation_longitude, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING {ATTESTATION_TOKEN_COLS}"
    ))
    .bind(user_id)
    .bind(soultoken_id)
    .bind(scope)
    .bind(token_hash)
    .bind(requesting_business_soultoken_id)
    .bind(user_device_id)
    .bind(presentation_latitude)
    .bind(presentation_longitude)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_token_by_hash(
    pool:       &PgPool,
    token_hash: &str,
) -> AppResult<Option<AttestationTokenRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_TOKEN_COLS} FROM attestation_tokens \
         WHERE token_hash = $1"
    ))
    .bind(token_hash)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_tokens_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<AttestationTokenRow>> {
    sqlx::query_as(&format!(
        "SELECT {ATTESTATION_TOKEN_COLS} FROM attestation_tokens \
         WHERE user_id = $1 ORDER BY issued_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn mark_token_verified(
    pool:     &PgPool,
    token_id: i32,
) -> AppResult<AttestationTokenRow> {
    sqlx::query_as(&format!(
        "UPDATE attestation_tokens \
         SET verified_at = now(), user_presented = true \
         WHERE id = $1 \
         RETURNING {ATTESTATION_TOKEN_COLS}"
    ))
    .bind(token_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn revoke_token(
    pool:     &PgPool,
    token_id: i32,
) -> AppResult<AttestationTokenRow> {
    sqlx::query_as(&format!(
        "UPDATE attestation_tokens SET revoked_at = now() \
         WHERE id = $1 \
         RETURNING {ATTESTATION_TOKEN_COLS}"
    ))
    .bind(token_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Verification attempts ─────────────────────────────────────────────────────

pub async fn record_verification_attempt(
    pool:                            &PgPool,
    token_hash:                      &str,
    attestation_token_id:            Option<i32>,
    requesting_business_soultoken_id: Option<i32>,
    request_signature:               Option<&str>,
    ip_address:                      Option<&str>,
    user_agent:                      Option<&str>,
    outcome:                         &str,
) -> AppResult<ThirdPartyVerificationAttemptRow> {
    sqlx::query_as(
        "INSERT INTO third_party_verification_attempts \
         (token_hash, attestation_token_id, requesting_business_soultoken_id, \
          request_signature, ip_address, user_agent, outcome) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id, token_hash, attestation_token_id, \
                   requesting_business_soultoken_id, request_signature, \
                   ip_address, user_agent, outcome, attempted_at"
    )
    .bind(token_hash)
    .bind(attestation_token_id)
    .bind(requesting_business_soultoken_id)
    .bind(request_signature)
    .bind(ip_address)
    .bind(user_agent)
    .bind(outcome)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

/// Count verification attempts from a business soultoken in the last N minutes.
pub async fn get_recent_attempts_by_business(
    pool:                            &PgPool,
    requesting_business_soultoken_id: i32,
    minutes:                         i32,
) -> AppResult<i32> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM third_party_verification_attempts \
         WHERE requesting_business_soultoken_id = $1 \
           AND attempted_at > now() - ($2 || ' minutes')::INTERVAL"
    )
    .bind(requesting_business_soultoken_id)
    .bind(minutes)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(count as i32)
}
