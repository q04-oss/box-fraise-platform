#![allow(missing_docs)]
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{
    PlatformConfigurationHistoryResponse, PlatformConfigurationHistoryRow,
    PlatformConfigurationRow, PLATFORM_CONFIG_COLS, DEFAULTS,
};

pub async fn get_all(pool: &PgPool) -> AppResult<Vec<PlatformConfigurationRow>> {
    sqlx::query_as(&format!(
        "SELECT {PLATFORM_CONFIG_COLS} FROM platform_configuration ORDER BY key ASC"
    ))
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_by_key(
    pool: &PgPool,
    key:  &str,
) -> AppResult<Option<PlatformConfigurationRow>> {
    sqlx::query_as(&format!(
        "SELECT {PLATFORM_CONFIG_COLS} FROM platform_configuration WHERE key = $1"
    ))
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_value(
    pool:       &PgPool,
    key:        &str,
    new_value:  &str,
    updated_by: i32,
) -> AppResult<PlatformConfigurationRow> {
    sqlx::query_as(&format!(
        "UPDATE platform_configuration \
         SET value = $2, updated_by = $3, updated_at = now() \
         WHERE key = $1 \
         RETURNING {PLATFORM_CONFIG_COLS}"
    ))
    .bind(key)
    .bind(new_value)
    .bind(updated_by)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn record_history(
    pool:             &PgPool,
    configuration_id: i32,
    previous_value:   &str,
    new_value:        &str,
    changed_by:       i32,
) -> AppResult<PlatformConfigurationHistoryRow> {
    sqlx::query_as(
        "INSERT INTO platform_configuration_history \
         (configuration_id, previous_value, new_value, changed_by) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, configuration_id, previous_value, new_value, changed_by, changed_at"
    )
    .bind(configuration_id)
    .bind(previous_value)
    .bind(new_value)
    .bind(changed_by)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_history_by_key(
    pool: &PgPool,
    key:  &str,
) -> AppResult<Vec<PlatformConfigurationHistoryResponse>> {
    sqlx::query_as(
        "SELECT pc.key, pch.previous_value, pch.new_value, pch.changed_at \
         FROM platform_configuration_history pch \
         JOIN platform_configuration pc ON pch.configuration_id = pc.id \
         WHERE pc.key = $1 \
         ORDER BY pch.changed_at DESC"
    )
    .bind(key)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

/// Seed BFIP Section 15 defaults using ON CONFLICT DO NOTHING — safe to re-run.
pub async fn seed_defaults(pool: &PgPool) -> AppResult<()> {
    for (key, value, value_type, description) in DEFAULTS {
        sqlx::query(
            "INSERT INTO platform_configuration \
             (key, value, value_type, description, cache_ttl_seconds) \
             VALUES ($1, $2, $3, $4, 300) \
             ON CONFLICT (key) DO NOTHING"
        )
        .bind(key)
        .bind(value)
        .bind(value_type)
        .bind(description)
        .execute(pool)
        .await
        .map_err(DomainError::Db)?;
    }
    Ok(())
}
