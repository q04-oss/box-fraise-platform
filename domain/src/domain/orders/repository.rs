#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::error::{AppResult, DomainError};
use super::types::{OrderRow, VisitBoxRow, ORDER_COLS, VISIT_BOX_COLS};

// ── Orders ────────────────────────────────────────────────────────────────────

pub async fn create_order(
    pool:                &PgPool,
    user_id:             i32,
    business_id:         i32,
    variety_description: Option<&str>,
    box_count:           i32,
    amount_cents:        i32,
) -> AppResult<OrderRow> {
    sqlx::query_as(&format!(
        "INSERT INTO orders \
         (user_id, business_id, variety_description, box_count, amount_cents, \
          status, pickup_deadline) \
         VALUES ($1, $2, $3, $4, $5, 'pending', now() + INTERVAL '24 hours') \
         RETURNING {ORDER_COLS}"
    ))
    .bind(user_id)
    .bind(business_id)
    .bind(variety_description)
    .bind(box_count)
    .bind(amount_cents)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_order_by_id(
    pool:     &PgPool,
    order_id: i32,
) -> AppResult<Option<OrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {ORDER_COLS} FROM orders WHERE id = $1"
    ))
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_orders_by_user(
    pool:    &PgPool,
    user_id: i32,
) -> AppResult<Vec<OrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {ORDER_COLS} FROM orders WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_orders_by_business(
    pool:        &PgPool,
    business_id: i32,
) -> AppResult<Vec<OrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {ORDER_COLS} FROM orders \
         WHERE business_id = $1 AND status != 'cancelled' \
         ORDER BY created_at DESC"
    ))
    .bind(business_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn update_order_status(
    pool:     &PgPool,
    order_id: i32,
    status:   &str,
) -> AppResult<OrderRow> {
    sqlx::query_as(&format!(
        "UPDATE orders SET status = $2, updated_at = now() \
         WHERE id = $1 RETURNING {ORDER_COLS}"
    ))
    .bind(order_id)
    .bind(status)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn collect_order_db(
    pool:     &PgPool,
    order_id: i32,
    box_id:   i32,
) -> AppResult<OrderRow> {
    sqlx::query_as(&format!(
        "UPDATE orders SET \
         status                = 'collected', \
         collected_via_box_id  = $2, \
         updated_at            = now() \
         WHERE id = $1 RETURNING {ORDER_COLS}"
    ))
    .bind(order_id)
    .bind(box_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn cancel_order_db(
    pool:     &PgPool,
    order_id: i32,
) -> AppResult<OrderRow> {
    sqlx::query_as(&format!(
        "UPDATE orders SET status = 'cancelled', updated_at = now() \
         WHERE id = $1 RETURNING {ORDER_COLS}"
    ))
    .bind(order_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

/// Find a pending order for `user_id` at the business whose location matches
/// the visit's location. Used when the tapped box has no `assigned_order_id`.
pub async fn find_pending_order_for_visit(
    pool:     &PgPool,
    user_id:  i32,
    visit_id: i32,
) -> AppResult<Option<OrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {ORDER_COLS} FROM orders o
         WHERE o.user_id = $1
           AND o.status = 'pending'
           AND o.business_id IN (
               SELECT b.id FROM businesses b
               WHERE b.location_id = (
                   SELECT location_id FROM staff_visits WHERE id = $2
               )
               AND b.deleted_at IS NULL
           )
         ORDER BY o.created_at ASC LIMIT 1"
    ))
    .bind(user_id)
    .bind(visit_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

// ── Visit boxes ───────────────────────────────────────────────────────────────

pub async fn create_visit_box(
    pool:         &PgPool,
    visit_id:     i32,
    nfc_chip_uid: &str,
    quantity:     i32,
) -> AppResult<VisitBoxRow> {
    sqlx::query_as(&format!(
        "INSERT INTO visit_boxes (visit_id, nfc_chip_uid, quantity) \
         VALUES ($1, $2, $3) \
         RETURNING {VISIT_BOX_COLS}"
    ))
    .bind(visit_id)
    .bind(nfc_chip_uid)
    .bind(quantity)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_box_by_uid(
    pool:         &PgPool,
    nfc_chip_uid: &str,
) -> AppResult<Option<VisitBoxRow>> {
    sqlx::query_as(&format!(
        "SELECT {VISIT_BOX_COLS} FROM visit_boxes WHERE nfc_chip_uid = $1"
    ))
    .bind(nfc_chip_uid)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn get_boxes_by_visit(
    pool:     &PgPool,
    visit_id: i32,
) -> AppResult<Vec<VisitBoxRow>> {
    sqlx::query_as(&format!(
        "SELECT {VISIT_BOX_COLS} FROM visit_boxes WHERE visit_id = $1 ORDER BY created_at ASC"
    ))
    .bind(visit_id)
    .fetch_all(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn activate_box_db(
    pool:               &PgPool,
    box_id:             i32,
    delivery_signature: &str,
    expires_at:         DateTime<Utc>,
) -> AppResult<VisitBoxRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_boxes SET \
         activated_at       = now(), \
         delivery_signature = $2, \
         expires_at         = $3 \
         WHERE id = $1 RETURNING {VISIT_BOX_COLS}"
    ))
    .bind(box_id)
    .bind(delivery_signature)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}

/// Atomically tap a box by setting `tapped_at` and `tapped_by_user_id`.
///
/// `WHERE tapped_at IS NULL` enforces single-use at the database level.
/// Returns `None` if the box was already tapped — the caller should treat
/// this as a clone detection event.
pub async fn tap_box(
    pool:    &PgPool,
    box_id:  i32,
    user_id: i32,
) -> AppResult<Option<VisitBoxRow>> {
    sqlx::query_as(&format!(
        "UPDATE visit_boxes SET \
         tapped_by_user_id       = $2, \
         tapped_at               = now(), \
         collection_confirmed_at = now() \
         WHERE id = $1 AND tapped_at IS NULL \
         RETURNING {VISIT_BOX_COLS}"
    ))
    .bind(box_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(DomainError::Db)
}

pub async fn record_clone_detected(
    pool:   &PgPool,
    box_id: i32,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE visit_boxes SET \
         clone_detected    = true, \
         clone_detected_at = now() \
         WHERE id = $1"
    )
    .bind(box_id)
    .execute(pool)
    .await
    .map_err(DomainError::Db)?;
    Ok(())
}

pub async fn assign_box_to_order(
    pool:     &PgPool,
    box_id:   i32,
    order_id: i32,
) -> AppResult<VisitBoxRow> {
    sqlx::query_as(&format!(
        "UPDATE visit_boxes SET assigned_order_id = $2 \
         WHERE id = $1 RETURNING {VISIT_BOX_COLS}"
    ))
    .bind(box_id)
    .bind(order_id)
    .fetch_one(pool)
    .await
    .map_err(DomainError::Db)
}
