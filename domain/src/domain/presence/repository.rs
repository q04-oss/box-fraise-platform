#![allow(missing_docs)]
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{
    PresenceEventRow, PresenceSessionRow, PresenceThresholdRow,
    PRESENCE_EVENT_COLS, PRESENCE_SESSION_COLS, PRESENCE_THRESHOLD_COLS,
};

// ── Sessions ──────────────────────────────────────────────────────────────────

pub async fn create_presence_session(
    pool:               &PgPool,
    user_id:            i32,
    business_id:        i32,
    beacon_id:          Option<i32>,
    device_identifier:  Option<&str>,
    started_at:         DateTime<Utc>,
    ended_at:           DateTime<Utc>,
    total_dwell_minutes: Option<i32>,
) -> AppResult<PresenceSessionRow> {
    sqlx::query_as(&format!(
        "INSERT INTO presence_sessions \
         (user_id, business_id, beacon_id, device_identifier, started_at, \
          ended_at, total_dwell_minutes) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING {PRESENCE_SESSION_COLS}"
    ))
    .bind(user_id)
    .bind(business_id)
    .bind(beacon_id)
    .bind(device_identifier)
    .bind(started_at)
    .bind(ended_at)
    .bind(total_dwell_minutes)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Events ────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub async fn create_presence_event(
    pool:                   &PgPool,
    user_id:                i32,
    business_id:            i32,
    beacon_id:              Option<i32>,
    session_id:             Option<i32>,
    box_id:                 Option<i32>,
    event_type:             &str,
    rssi:                   Option<i32>,
    rssi_threshold_applied: Option<i32>,
    dwell_start_at:         Option<DateTime<Utc>>,
    dwell_end_at:           Option<DateTime<Utc>>,
    dwell_minutes:          Option<i32>,
    is_qualifying:          bool,
    rejection_reason:       Option<&str>,
    app_attest_assertion:   Option<&str>,
    beacon_witness_hmac:    Option<&str>,
    hardware_identifier:    Option<&str>,
    calendar_date:          NaiveDate,
) -> AppResult<PresenceEventRow> {
    sqlx::query_as(&format!(
        "INSERT INTO presence_events \
         (user_id, business_id, beacon_id, session_id, box_id, event_type, \
          rssi, rssi_threshold_applied, dwell_start_at, dwell_end_at, dwell_minutes, \
          is_qualifying, rejection_reason, app_attest_assertion, beacon_witness_hmac, \
          hardware_identifier, calendar_date) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
         RETURNING {PRESENCE_EVENT_COLS}"
    ))
    .bind(user_id)
    .bind(business_id)
    .bind(beacon_id)
    .bind(session_id)
    .bind(box_id)
    .bind(event_type)
    .bind(rssi)
    .bind(rssi_threshold_applied)
    .bind(dwell_start_at)
    .bind(dwell_end_at)
    .bind(dwell_minutes)
    .bind(is_qualifying)
    .bind(rejection_reason)
    .bind(app_attest_assertion)
    .bind(beacon_witness_hmac)
    .bind(hardware_identifier)
    .bind(calendar_date)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_presence_events_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<PresenceEventRow>> {
    sqlx::query_as(&format!(
        "SELECT {PRESENCE_EVENT_COLS} FROM presence_events \
         WHERE user_id = $1 ORDER BY occurred_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Thresholds ────────────────────────────────────────────────────────────────

/// Atomically create or touch a presence threshold record for a user.
/// ON CONFLICT (user_id): one threshold per user — if exists, update updated_at only.
pub async fn get_or_create_threshold(
    pool:        &PgPool,
    user_id:     i32,
    business_id: i32,
) -> AppResult<PresenceThresholdRow> {
    sqlx::query_as(&format!(
        "INSERT INTO presence_thresholds (user_id, business_id, started_at) \
         VALUES ($1, $2, now()) \
         ON CONFLICT (user_id) DO UPDATE SET updated_at = now() \
         RETURNING {PRESENCE_THRESHOLD_COLS}"
    ))
    .bind(user_id)
    .bind(business_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_threshold(
    pool:                     &PgPool,
    threshold_id:             i32,
    event_count:              i32,
    days_count:               i32,
    last_qualifying_event_at: Option<DateTime<Utc>>,
    threshold_met_at:         Option<DateTime<Utc>>,
) -> AppResult<PresenceThresholdRow> {
    sqlx::query_as(&format!(
        "UPDATE presence_thresholds \
         SET event_count = $2, days_count = $3, \
             last_qualifying_event_at = $4, threshold_met_at = $5, \
             updated_at = now() \
         WHERE id = $1 \
         RETURNING {PRESENCE_THRESHOLD_COLS}"
    ))
    .bind(threshold_id)
    .bind(event_count)
    .bind(days_count)
    .bind(last_qualifying_event_at)
    .bind(threshold_met_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_threshold_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Option<PresenceThresholdRow>> {
    sqlx::query_as(&format!(
        "SELECT {PRESENCE_THRESHOLD_COLS} FROM presence_thresholds \
         WHERE user_id = $1"
    ))
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Qualifying events ─────────────────────────────────────────────────────────

/// Link a qualifying presence event to a threshold. Idempotent — ON CONFLICT DO NOTHING.
pub async fn record_qualifying_event(
    pool:             &PgPool,
    threshold_id:     i32,
    presence_event_id: i32,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO qualifying_presence_events (threshold_id, presence_event_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING"
    )
    .bind(threshold_id)
    .bind(presence_event_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}

/// Fetch all presence events that have been linked to a threshold.
pub async fn get_qualifying_events(
    pool:         &PgPool,
    threshold_id: i32,
) -> AppResult<Vec<PresenceEventRow>> {
    // Columns must be prefixed with pe. to disambiguate from qpe.id in the JOIN.
    sqlx::query_as(
        "SELECT pe.id, pe.user_id, pe.business_id, pe.beacon_id, pe.session_id, pe.box_id, \
                pe.event_type, pe.rssi, pe.rssi_threshold_applied, \
                pe.dwell_start_at, pe.dwell_end_at, pe.dwell_minutes, \
                pe.is_qualifying, pe.rejection_reason, pe.app_attest_assertion, \
                pe.beacon_witness_hmac, pe.hardware_identifier, \
                pe.calendar_date, pe.occurred_at \
         FROM presence_events pe \
         JOIN qualifying_presence_events qpe ON qpe.presence_event_id = pe.id \
         WHERE qpe.threshold_id = $1 \
         ORDER BY pe.occurred_at"
    )
    .bind(threshold_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}
