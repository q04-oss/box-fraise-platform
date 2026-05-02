#![allow(missing_docs)]
use chrono::NaiveDate;
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{
    BeaconHealthLogRow, BeaconRotationLogRow, BeaconRow, BeaconSummaryRow,
    BEACON_COLS, BEACON_COLS_WITH_SECRET,
};

// ── Beacons ───────────────────────────────────────────────────────────────────

/// Insert a new beacon and return the full row including secret_key.
pub async fn create_beacon(
    pool:                  &PgPool,
    location_id:           i32,
    business_id:           i32,
    secret_key:            &str,
    minimum_rssi_threshold: i32,
) -> AppResult<BeaconRow> {
    sqlx::query_as(&format!(
        "INSERT INTO beacons (location_id, business_id, secret_key, minimum_rssi_threshold) \
         VALUES ($1, $2, $3, $4) \
         RETURNING {BEACON_COLS_WITH_SECRET}"
    ))
    .bind(location_id)
    .bind(business_id)
    .bind(secret_key)
    .bind(minimum_rssi_threshold)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

/// Fetch a beacon by ID including secret_key — for HMAC derivation only.
pub async fn get_beacon_by_id(pool: &PgPool, beacon_id: i32) -> AppResult<Option<BeaconRow>> {
    sqlx::query_as(&format!(
        "SELECT {BEACON_COLS_WITH_SECRET} FROM beacons \
         WHERE id = $1 AND is_active = true"
    ))
    .bind(beacon_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

/// Fetch all active beacons for a business. Does NOT include secret_key.
pub async fn get_beacons_by_business(
    pool:        &PgPool,
    business_id: i32,
) -> AppResult<Vec<BeaconSummaryRow>> {
    sqlx::query_as(&format!(
        "SELECT {BEACON_COLS} FROM beacons \
         WHERE business_id = $1 AND is_active = true \
         ORDER BY created_at DESC"
    ))
    .bind(business_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

/// Rotate the secret key: old key preserved as previous_secret_key for the 24-hour
/// grace period. Returns the updated full row.
pub async fn rotate_secret_key(
    pool:           &PgPool,
    beacon_id:      i32,
    new_secret_key: &str,
) -> AppResult<BeaconRow> {
    sqlx::query_as(&format!(
        "UPDATE beacons \
         SET previous_secret_key = secret_key, \
             secret_key          = $2, \
             key_rotated_at      = now(), \
             updated_at          = now() \
         WHERE id = $1 \
         RETURNING {BEACON_COLS_WITH_SECRET}"
    ))
    .bind(beacon_id)
    .bind(new_secret_key)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Beacon rotation log ───────────────────────────────────────────────────────

/// Record today's expected UUID hash in the rotation log.
/// ON CONFLICT DO NOTHING — idempotent for repeated calls on the same day.
/// Returns None if a row already existed (conflict path).
pub async fn record_rotation(
    pool:               &PgPool,
    beacon_id:          i32,
    calendar_date:      NaiveDate,
    expected_uuid_hash: &str,
) -> AppResult<Option<BeaconRotationLogRow>> {
    sqlx::query_as(
        "INSERT INTO beacon_rotation_log (beacon_id, calendar_date, expected_uuid_hash) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (beacon_id, calendar_date) DO NOTHING \
         RETURNING id, beacon_id, calendar_date, expected_uuid_hash, \
                   first_seen_at, rotation_status, created_at"
    )
    .bind(beacon_id)
    .bind(calendar_date)
    .bind(expected_uuid_hash)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Beacon health log ─────────────────────────────────────────────────────────

/// Record a health check result for a beacon.
pub async fn record_health(
    pool:            &PgPool,
    beacon_id:       i32,
    is_responding:   bool,
    signal_strength: Option<i32>,
) -> AppResult<BeaconHealthLogRow> {
    sqlx::query_as(
        "INSERT INTO beacon_health_log (beacon_id, is_responding, signal_strength) \
         VALUES ($1, $2, $3) \
         RETURNING id, beacon_id, checked_at, is_responding, signal_strength, firmware_version"
    )
    .bind(beacon_id)
    .bind(is_responding)
    .bind(signal_strength)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}
