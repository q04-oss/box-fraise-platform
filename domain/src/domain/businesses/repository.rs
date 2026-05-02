#![allow(missing_docs)] // Repository layer — internal implementation, documented at service layer.
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{BusinessRow, LocationRow, BUSINESS_COLS, LOCATION_COLS};

// ── Locations ─────────────────────────────────────────────────────────────────

/// Insert a new location and return the created row.
pub async fn create_location(
    pool:          &PgPool,
    name:          &str,
    location_type: &str,
    address:       &str,
    latitude:      Option<f64>,
    longitude:     Option<f64>,
    timezone:      &str,
    contact_email: Option<&str>,
    contact_phone: Option<&str>,
) -> AppResult<LocationRow> {
    sqlx::query_as(&format!(
        "INSERT INTO locations \
         (name, location_type, address, latitude, longitude, timezone, \
          contact_email, contact_phone) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING {LOCATION_COLS}"
    ))
    .bind(name)
    .bind(location_type)
    .bind(address)
    .bind(latitude)
    .bind(longitude)
    .bind(timezone)
    .bind(contact_email)
    .bind(contact_phone)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_location_by_id(pool: &PgPool, location_id: i32) -> AppResult<Option<LocationRow>> {
    sqlx::query_as(&format!(
        "SELECT {LOCATION_COLS} FROM locations WHERE id = $1 AND is_active = true"
    ))
    .bind(location_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Businesses ────────────────────────────────────────────────────────────────

/// Insert a new business in `pending` status and return the created row.
pub async fn create_business(
    pool:             &PgPool,
    location_id:      i32,
    primary_holder_id: i32,
    name:             &str,
) -> AppResult<BusinessRow> {
    sqlx::query_as(&format!(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, $3, 'pending') \
         RETURNING {BUSINESS_COLS}"
    ))
    .bind(location_id)
    .bind(primary_holder_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_business_by_id(pool: &PgPool, business_id: i32) -> AppResult<Option<BusinessRow>> {
    sqlx::query_as(&format!(
        "SELECT {BUSINESS_COLS} FROM businesses \
         WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_businesses_by_holder(pool: &PgPool, user_id: i32) -> AppResult<Vec<BusinessRow>> {
    sqlx::query_as(&format!(
        "SELECT {BUSINESS_COLS} FROM businesses \
         WHERE primary_holder_id = $1 AND deleted_at IS NULL \
         ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

/// Count active businesses owned by a user (for the 5-business abuse cap).
pub async fn count_active_businesses(pool: &PgPool, user_id: i32) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM businesses \
         WHERE primary_holder_id = $1 AND is_active = true AND deleted_at IS NULL"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}
