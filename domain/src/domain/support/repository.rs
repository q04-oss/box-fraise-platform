#![allow(missing_docs)]
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{GiftBoxHistoryRow, SupportBookingRow, SUPPORT_BOOKING_COLS};

// ── Support bookings ──────────────────────────────────────────────────────────

pub async fn create_booking(
    pool:              &PgPool,
    visit_id:          i32,
    user_id:           i32,
    issue_description: Option<&str>,
    priority:          &str,
) -> AppResult<SupportBookingRow> {
    sqlx::query_as(&format!(
        "INSERT INTO support_bookings \
         (visit_id, user_id, issue_description, priority, status) \
         VALUES ($1, $2, $3, $4, 'booked') \
         RETURNING {SUPPORT_BOOKING_COLS}"
    ))
    .bind(visit_id)
    .bind(user_id)
    .bind(issue_description)
    .bind(priority)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db) = e {
            if db.constraint() == Some("idx_support_bookings_one_active_per_visit") {
                return DomainError::Conflict(
                    "user already has an active booking at this visit".to_string(),
                );
            }
        }
        DomainError::Db(e)
    })
}

pub async fn get_booking_by_id(
    pool:       &PgPool,
    booking_id: i32,
) -> AppResult<Option<SupportBookingRow>> {
    sqlx::query_as(&format!(
        "SELECT {SUPPORT_BOOKING_COLS} FROM support_bookings WHERE id = $1"
    ))
    .bind(booking_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_bookings_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<SupportBookingRow>> {
    sqlx::query_as(&format!(
        "SELECT {SUPPORT_BOOKING_COLS} FROM support_bookings \
         WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_bookings_by_visit(
    pool:     &PgPool,
    visit_id: i32,
) -> AppResult<Vec<SupportBookingRow>> {
    sqlx::query_as(&format!(
        "SELECT {SUPPORT_BOOKING_COLS} FROM support_bookings \
         WHERE visit_id = $1 ORDER BY created_at ASC"
    ))
    .bind(visit_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn attend_booking(
    pool:       &PgPool,
    booking_id: i32,
) -> AppResult<SupportBookingRow> {
    sqlx::query_as(&format!(
        "UPDATE support_bookings \
         SET status = 'attended', attended_at = now(), updated_at = now() \
         WHERE id = $1 \
         RETURNING {SUPPORT_BOOKING_COLS}"
    ))
    .bind(booking_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn resolve_booking(
    pool:                   &PgPool,
    booking_id:             i32,
    resolution_description: &str,
    resolution_signature:   &str,
    resolution_staff_id:    i32,
    gift_box_provided:      bool,
) -> AppResult<SupportBookingRow> {
    sqlx::query_as(&format!(
        "UPDATE support_bookings \
         SET status = 'resolved', \
             resolved_at = now(), \
             resolution_description = $2, \
             resolution_signature = $3, \
             resolution_staff_id = $4, \
             gift_box_provided = $5, \
             updated_at = now() \
         WHERE id = $1 \
         RETURNING {SUPPORT_BOOKING_COLS}"
    ))
    .bind(booking_id)
    .bind(resolution_description)
    .bind(resolution_signature)
    .bind(resolution_staff_id)
    .bind(gift_box_provided)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn cancel_booking(
    pool:                &PgPool,
    booking_id:          i32,
    cancellation_reason: &str,
) -> AppResult<SupportBookingRow> {
    sqlx::query_as(&format!(
        "UPDATE support_bookings \
         SET status = 'cancelled', cancelled_at = now(), \
             cancellation_reason = $2, updated_at = now() \
         WHERE id = $1 \
         RETURNING {SUPPORT_BOOKING_COLS}"
    ))
    .bind(booking_id)
    .bind(cancellation_reason)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn mark_confirmation_sent(
    pool:       &PgPool,
    booking_id: i32,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE support_bookings \
         SET booking_confirmation_sent_at = now(), updated_at = now() \
         WHERE id = $1"
    )
    .bind(booking_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}

pub async fn active_booking_count_for_visit(
    pool:     &PgPool,
    visit_id: i32,
) -> AppResult<i64> {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM support_bookings \
         WHERE visit_id = $1 AND status NOT IN ('cancelled', 'no_show')"
    )
    .bind(visit_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Gift box history ──────────────────────────────────────────────────────────

pub async fn record_gift_box(
    pool:        &PgPool,
    user_id:     i32,
    visit_id:    i32,
    box_id:      Option<i32>,
    gift_reason: &str,
    covered_by:  &str,
) -> AppResult<GiftBoxHistoryRow> {
    sqlx::query_as(
        "INSERT INTO gift_box_history \
         (user_id, visit_id, box_id, gift_reason, covered_by) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, user_id, visit_id, box_id, gift_reason, covered_by, gifted_at"
    )
    .bind(user_id)
    .bind(visit_id)
    .bind(box_id)
    .bind(gift_reason)
    .bind(covered_by)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_last_platform_gift(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Option<GiftBoxHistoryRow>> {
    sqlx::query_as(
        "SELECT id, user_id, visit_id, box_id, gift_reason, covered_by, gifted_at \
         FROM gift_box_history \
         WHERE user_id = $1 AND covered_by = 'platform' \
         ORDER BY gifted_at DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn check_platform_gift_eligible(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<bool> {
    let eligible_after: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT platform_gift_eligible_after FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)?;

    Ok(eligible_after.map(|dt| dt <= chrono::Utc::now()).unwrap_or(true))
}

pub async fn update_platform_gift_eligible_after(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE users \
         SET platform_gift_eligible_after = now() + INTERVAL '6 months' \
         WHERE id = $1"
    )
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}
