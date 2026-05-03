#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppResult, DomainError};
use super::types::{
    SoultokenRenewalRow, SoultokenRow,
    SOULTOKEN_COLS, SOULTOKEN_RENEWAL_COLS,
};

// ── Soultokens ────────────────────────────────────────────────────────────────

pub async fn create_soultoken(
    pool:                        &PgPool,
    uuid:                        Uuid,
    display_code:                &str,
    display_code_key_version:    i32,
    holder_user_id:              i32,
    token_type:                  &str,
    business_id:                 Option<i32>,
    identity_credential_id:      Option<i32>,
    presence_threshold_id:       Option<i32>,
    attestation_id:              Option<i32>,
    signature:                   Option<&str>,
    expires_at:                  DateTime<Utc>,
) -> AppResult<SoultokenRow> {
    sqlx::query_as(&format!(
        "INSERT INTO soultokens \
         (uuid, display_code, display_code_key_version, holder_user_id, token_type, \
          business_id, identity_credential_id, presence_threshold_id, attestation_id, \
          signature, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
         RETURNING {SOULTOKEN_COLS}"
    ))
    .bind(uuid)
    .bind(display_code)
    .bind(display_code_key_version)
    .bind(holder_user_id)
    .bind(token_type)
    .bind(business_id)
    .bind(identity_credential_id)
    .bind(presence_threshold_id)
    .bind(attestation_id)
    .bind(signature)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_soultoken_by_id(
    pool:        &PgPool,
    soultoken_id: i32,
) -> AppResult<Option<SoultokenRow>> {
    sqlx::query_as(&format!(
        "SELECT {SOULTOKEN_COLS} FROM soultokens WHERE id = $1"
    ))
    .bind(soultoken_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_soultoken_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Option<SoultokenRow>> {
    sqlx::query_as(&format!(
        "SELECT {SOULTOKEN_COLS} FROM soultokens \
         WHERE holder_user_id = $1 \
           AND token_type = 'user' \
           AND revoked_at IS NULL \
         ORDER BY issued_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_active_soultoken_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Option<SoultokenRow>> {
    sqlx::query_as(&format!(
        "SELECT {SOULTOKEN_COLS} FROM soultokens \
         WHERE holder_user_id = $1 \
           AND token_type = 'user' \
           AND revoked_at IS NULL \
           AND expires_at > now() \
         ORDER BY issued_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn revoke_soultoken(
    pool:                  &PgPool,
    soultoken_id:          i32,
    revocation_reason:     &str,
    revocation_staff_id:   Option<i32>,
    revocation_visit_id:   Option<i32>,
    surrender_witnessed_by: Option<i32>,
) -> AppResult<SoultokenRow> {
    sqlx::query_as(&format!(
        "UPDATE soultokens SET \
         revoked_at             = now(), \
         revocation_reason      = $2, \
         revocation_staff_id    = $3, \
         revocation_visit_id    = $4, \
         surrender_witnessed_by = $5 \
         WHERE id = $1 \
         RETURNING {SOULTOKEN_COLS}"
    ))
    .bind(soultoken_id)
    .bind(revocation_reason)
    .bind(revocation_staff_id)
    .bind(revocation_visit_id)
    .bind(surrender_witnessed_by)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_user_soultoken_id(
    pool:         &PgPool,
    user_id:      i32,
    soultoken_id: Option<i32>,
) -> AppResult<()> {
    sqlx::query("UPDATE users SET soultoken_id = $2 WHERE id = $1")
        .bind(user_id)
        .bind(soultoken_id)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    Ok(())
}

// ── Soultoken renewals ────────────────────────────────────────────────────────

pub async fn create_renewal(
    pool:                  &PgPool,
    soultoken_id:          i32,
    user_id:               i32,
    triggering_presence_id: Option<i32>,
    renewal_type:          &str,
    previous_expires_at:   DateTime<Utc>,
    new_expires_at:        DateTime<Utc>,
) -> AppResult<SoultokenRenewalRow> {
    sqlx::query_as(&format!(
        "INSERT INTO soultoken_renewals \
         (soultoken_id, user_id, triggering_presence_id, renewal_type, \
          previous_expires_at, new_expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         RETURNING {SOULTOKEN_RENEWAL_COLS}"
    ))
    .bind(soultoken_id)
    .bind(user_id)
    .bind(triggering_presence_id)
    .bind(renewal_type)
    .bind(previous_expires_at)
    .bind(new_expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn renew_soultoken(
    pool:          &PgPool,
    soultoken_id:  i32,
    new_expires_at: DateTime<Utc>,
) -> AppResult<SoultokenRow> {
    sqlx::query_as(&format!(
        "UPDATE soultokens SET \
         expires_at      = $2, \
         last_renewed_at = now() \
         WHERE id = $1 \
         RETURNING {SOULTOKEN_COLS}"
    ))
    .bind(soultoken_id)
    .bind(new_expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}
