use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

// ── Database rows ─────────────────────────────────────────────────────────────

#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PresenceSessionRow {
    pub id:                              i32,
    pub user_id:                         i32,
    pub business_id:                     i32,
    pub beacon_id:                       Option<i32>,
    pub visit_id:                        Option<i32>,
    pub device_identifier:               Option<String>,
    pub device_attestation_verified:     bool,
    pub device_attestation_verified_at:  Option<DateTime<Utc>>,
    pub started_at:                      DateTime<Utc>,
    pub ended_at:                        Option<DateTime<Utc>>,
    pub total_dwell_minutes:             Option<i32>,
    pub is_qualifying:                   bool,
    pub rejection_reason:                Option<String>,
    pub contributed_to_threshold_id:     Option<i32>,
    pub created_at:                      DateTime<Utc>,
}

#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PresenceEventRow {
    pub id:                      i32,
    pub user_id:                 i32,
    pub business_id:             i32,
    pub beacon_id:               Option<i32>,
    pub session_id:              Option<i32>,
    pub box_id:                  Option<i32>,
    pub event_type:              String,
    pub rssi:                    Option<i32>,
    pub rssi_threshold_applied:  Option<i32>,
    pub dwell_start_at:          Option<DateTime<Utc>>,
    pub dwell_end_at:            Option<DateTime<Utc>>,
    pub dwell_minutes:           Option<i32>,
    pub is_qualifying:           bool,
    pub rejection_reason:        Option<String>,
    pub app_attest_assertion:    Option<String>,
    pub beacon_witness_hmac:     Option<String>,
    pub hardware_identifier:     Option<String>,
    pub calendar_date:           NaiveDate,
    pub occurred_at:             DateTime<Utc>,
}

#[allow(missing_docs)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PresenceThresholdRow {
    pub id:                       i32,
    pub user_id:                  i32,
    pub business_id:              i32,
    pub event_count:              i32,
    pub days_count:               i32,
    pub started_at:               Option<DateTime<Utc>>,
    pub last_qualifying_event_at: Option<DateTime<Utc>>,
    pub threshold_met_at:         Option<DateTime<Utc>>,
    pub updated_at:               DateTime<Utc>,
}

// ── Column lists ──────────────────────────────────────────────────────────────

/// All columns of `presence_sessions` for SELECT queries.
pub const PRESENCE_SESSION_COLS: &str =
    "id, user_id, business_id, beacon_id, visit_id, device_identifier, \
     device_attestation_verified, device_attestation_verified_at, \
     started_at, ended_at, total_dwell_minutes, is_qualifying, rejection_reason, \
     contributed_to_threshold_id, created_at";

/// All columns of `presence_events` for SELECT queries.
pub const PRESENCE_EVENT_COLS: &str =
    "id, user_id, business_id, beacon_id, session_id, box_id, event_type, \
     rssi, rssi_threshold_applied, dwell_start_at, dwell_end_at, dwell_minutes, \
     is_qualifying, rejection_reason, app_attest_assertion, beacon_witness_hmac, \
     hardware_identifier, calendar_date, occurred_at";

/// All columns of `presence_thresholds` for SELECT queries.
pub const PRESENCE_THRESHOLD_COLS: &str =
    "id, user_id, business_id, event_count, days_count, started_at, \
     last_qualifying_event_at, threshold_met_at, updated_at";

// ── Request bodies ────────────────────────────────────────────────────────────

/// Request body for `POST /api/presence/beacon-dwell`.
#[allow(missing_docs)]
#[derive(Debug, Deserialize)]
pub struct RecordBeaconDwellRequest {
    pub beacon_id:             i32,
    pub business_id:           i32,
    pub rssi:                  i32,
    pub dwell_minutes:         i32,
    pub beacon_witness_hmac:   String,
    pub app_attest_assertion:  Option<String>,
    pub device_identifier:     Option<String>,
    pub started_at:            DateTime<Utc>,
    pub ended_at:              DateTime<Utc>,
}

/// Request body for `POST /api/presence/nfc-tap`.
#[allow(missing_docs)]
#[derive(Debug, Deserialize)]
pub struct RecordNfcTapRequest {
    pub box_id:               i32,
    pub business_id:          i32,
    pub beacon_witness_hmac:  String,
    pub app_attest_assertion: Option<String>,
    pub device_identifier:    Option<String>,
}

// ── Response bodies ───────────────────────────────────────────────────────────

/// Summary of a single qualifying presence event within a PresenceStatusResponse.
#[allow(missing_docs)]
#[derive(Debug, Serialize)]
pub struct PresenceEventSummary {
    pub id:               i32,
    pub event_type:       String,
    pub calendar_date:    String,
    pub is_qualifying:    bool,
    pub rejection_reason: Option<String>,
    pub occurred_at:      DateTime<Utc>,
}

/// Response from all presence endpoints — reflects the current threshold state.
#[allow(missing_docs)]
#[derive(Debug, Serialize)]
pub struct PresenceStatusResponse {
    pub user_id:          i32,
    pub business_id:      i32,
    pub event_count:      i32,
    pub days_count:       i32,
    pub threshold_met:    bool,
    pub threshold_met_at: Option<DateTime<Utc>>,
    pub qualifying_events: Vec<PresenceEventSummary>,
}
