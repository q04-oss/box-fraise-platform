#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{BackgroundCheckRow, BACKGROUND_CHECK_COLS};

pub async fn create_check(
    pool:                   &PgPool,
    user_id:                i32,
    identity_credential_id: i32,
    provider:               &str,
    check_type:             &str,
) -> AppResult<BackgroundCheckRow> {
    sqlx::query_as(&format!(
        "INSERT INTO background_checks \
         (user_id, identity_credential_id, provider, check_type, status) \
         VALUES ($1, $2, $3, $4, 'pending') \
         RETURNING {BACKGROUND_CHECK_COLS}"
    ))
    .bind(user_id)
    .bind(identity_credential_id)
    .bind(provider)
    .bind(check_type)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_checks_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<BackgroundCheckRow>> {
    sqlx::query_as(&format!(
        "SELECT {BACKGROUND_CHECK_COLS} FROM background_checks \
         WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_check_by_external_id(
    pool:              &PgPool,
    external_check_id: &str,
) -> AppResult<Option<BackgroundCheckRow>> {
    sqlx::query_as(&format!(
        "SELECT {BACKGROUND_CHECK_COLS} FROM background_checks \
         WHERE external_check_id = $1"
    ))
    .bind(external_check_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_check_result(
    pool:              &PgPool,
    check_id:          i32,
    status:            &str,
    external_check_id: Option<&str>,
    response_hash:     Option<&str>,
    checked_at:        Option<DateTime<Utc>>,
    expires_at:        Option<DateTime<Utc>>,
) -> AppResult<BackgroundCheckRow> {
    sqlx::query_as(&format!(
        "UPDATE background_checks SET \
         status            = $2, \
         external_check_id = $3, \
         response_hash     = $4, \
         checked_at        = $5, \
         expires_at        = $6 \
         WHERE id = $1 \
         RETURNING {BACKGROUND_CHECK_COLS}"
    ))
    .bind(check_id)
    .bind(status)
    .bind(external_check_id)
    .bind(response_hash)
    .bind(checked_at)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_latest_check_by_type(
    pool:       &PgPool,
    user_id:    i32,
    check_type: &str,
) -> AppResult<Option<BackgroundCheckRow>> {
    sqlx::query_as(&format!(
        "SELECT {BACKGROUND_CHECK_COLS} FROM background_checks \
         WHERE user_id = $1 AND check_type = $2 \
         ORDER BY created_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .bind(check_type)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}
