use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

/// Full row from the `beacons` table, including secret fields.
/// Only fetched when secret_key is needed for HMAC derivation.
#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BeaconRow {
    pub id:                     i32,
    pub location_id:            i32,
    pub business_id:            Option<i32>,
    pub secret_key:             String,
    pub previous_secret_key:    Option<String>,
    pub key_rotated_at:         Option<DateTime<Utc>>,
    pub hardware_key_id:        Option<String>,
    pub minimum_rssi_threshold: i32,
    pub is_active:              bool,
    pub last_seen_at:           Option<DateTime<Utc>>,
    pub last_rotation_at:       Option<DateTime<Utc>>,
    pub failure_count:          i32,
    pub updated_at:             DateTime<Utc>,
    pub created_at:             DateTime<Utc>,
}

/// Public summary row from the `beacons` table.
/// Secret fields are excluded — used for list queries and responses.
#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BeaconSummaryRow {
    pub id:                     i32,
    pub location_id:            i32,
    pub business_id:            Option<i32>,
    pub key_rotated_at:         Option<DateTime<Utc>>,
    pub hardware_key_id:        Option<String>,
    pub minimum_rssi_threshold: i32,
    pub is_active:              bool,
    pub last_seen_at:           Option<DateTime<Utc>>,
    pub last_rotation_at:       Option<DateTime<Utc>>,
    pub failure_count:          i32,
    pub updated_at:             DateTime<Utc>,
    pub created_at:             DateTime<Utc>,
}

/// Full row from the `beacon_rotation_log` table.
#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BeaconRotationLogRow {
    pub id:                 i32,
    pub beacon_id:          i32,
    pub calendar_date:      NaiveDate,
    pub expected_uuid_hash: String,
    pub first_seen_at:      Option<DateTime<Utc>>,
    pub rotation_status:    String,
    pub created_at:         DateTime<Utc>,
}

/// Full row from the `beacon_health_log` table.
#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BeaconHealthLogRow {
    pub id:               i32,
    pub beacon_id:        i32,
    pub checked_at:       DateTime<Utc>,
    pub is_responding:    bool,
    pub signal_strength:  Option<i32>,
    pub firmware_version: Option<String>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

/// Columns for `BeaconSummaryRow` — excludes `secret_key` and `previous_secret_key`.
/// Used for list queries where secret fields must not be returned.
pub const BEACON_COLS: &str =
    "id, location_id, business_id, key_rotated_at, hardware_key_id, \
     minimum_rssi_threshold, is_active, last_seen_at, last_rotation_at, \
     failure_count, updated_at, created_at";

/// Columns for `BeaconRow` — includes secret fields.
/// Only used when secret_key is needed for HMAC derivation.
pub const BEACON_COLS_WITH_SECRET: &str =
    "id, location_id, business_id, secret_key, previous_secret_key, \
     key_rotated_at, hardware_key_id, minimum_rssi_threshold, is_active, \
     last_seen_at, last_rotation_at, failure_count, updated_at, created_at";

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/beacons`.
#[derive(Debug, Deserialize)]
pub struct CreateBeaconRequest {
    /// The business this beacon belongs to.
    pub business_id: i32,
    /// The location where this beacon is physically installed.
    pub location_id: i32,
    /// Minimum RSSI signal strength in dBm (default: -70).
    pub minimum_rssi_threshold: Option<i32>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Response returned by beacon endpoints. Never includes secret_key.
#[derive(Debug, Serialize)]
pub struct BeaconResponse {
    /// Beacon database ID.
    pub id:                     i32,
    /// Business this beacon belongs to.
    pub business_id:            Option<i32>,
    /// Location where this beacon is installed.
    pub location_id:            i32,
    /// Minimum RSSI threshold in dBm.
    pub minimum_rssi_threshold: i32,
    /// Whether this beacon is active.
    pub is_active:              bool,
    /// When this beacon was last detected.
    pub last_seen_at:           Option<DateTime<Utc>>,
    /// When this beacon record was created.
    pub created_at:             DateTime<Utc>,
}

/// Daily UUID response for `GET /api/beacons/:id/daily-uuid`.
#[derive(Debug, Serialize)]
pub struct DailyUuidResponse {
    /// Beacon database ID.
    pub beacon_id:     i32,
    /// The calendar date this UUID is valid for (YYYY-MM-DD).
    pub calendar_date: String,
    /// HMAC-derived UUID for today (UTC).
    pub uuid:          String,
    /// ISO 8601 timestamp when this UUID expires (end of UTC day).
    pub valid_until:   String,
}
