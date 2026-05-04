#![allow(missing_docs)]
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{AuditRequestLogRow, VerificationEventRow, VERIFICATION_EVENT_COLS};

// ── Verification events ───────────────────────────────────────────────────────

pub async fn get_events_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<VerificationEventRow>> {
    sqlx::query_as(&format!(
        "SELECT {VERIFICATION_EVENT_COLS} FROM verification_events \
         WHERE user_id = $1 ORDER BY created_at ASC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_events_by_user_and_type(
    pool:       &PgPool,
    user_id:    i32,
    event_type: &str,
) -> AppResult<Vec<VerificationEventRow>> {
    sqlx::query_as(&format!(
        "SELECT {VERIFICATION_EVENT_COLS} FROM verification_events \
         WHERE user_id = $1 AND event_type = $2 ORDER BY created_at ASC"
    ))
    .bind(user_id)
    .bind(event_type)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Audit request log ─────────────────────────────────────────────────────────

pub async fn record_audit_request(
    pool:            &PgPool,
    user_id:         i32,
    requested_by:    i32,
    delivery_method: &str,
) -> AppResult<AuditRequestLogRow> {
    sqlx::query_as(
        "INSERT INTO audit_request_log (user_id, requested_by, delivery_method) \
         VALUES ($1, $2, $3) \
         RETURNING id, user_id, requested_by, delivery_method, requested_at"
    )
    .bind(user_id)
    .bind(requested_by)
    .bind(delivery_method)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_audit_requests_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<AuditRequestLogRow>> {
    sqlx::query_as(
        "SELECT id, user_id, requested_by, delivery_method, requested_at \
         FROM audit_request_log WHERE user_id = $1 ORDER BY requested_at DESC"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}
