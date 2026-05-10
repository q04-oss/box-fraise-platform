#![allow(missing_docs)]
use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::error::{AppResult, DomainError};
use super::types::{
    WhiskedOrderItemRow, WhiskedOrderRow,
    WHISKED_ORDER_COLS, WHISKED_ORDER_ITEM_COLS,
};

// ── Orders ───────────────────────────────────────────────────────────────────

/// Insert a new order with the supplied pickup code and (optional) Stripe
/// PaymentIntent id. May fail with a unique-violation if `pickup_code`
/// collides with another active order's code; the service layer retries
/// with a fresh code on that error.
pub async fn create_order(
    conn:                     &mut PgConnection,
    user_id:                  i32,
    business_id:              i32,
    total_cents:              i32,
    pickup_code:              &str,
    stripe_payment_intent_id: Option<&str>,
) -> Result<WhiskedOrderRow, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO whisked_orders \
         (user_id, business_id, status, total_cents, pickup_code, stripe_payment_intent_id) \
         VALUES ($1, $2, 'pending', $3, $4, $5) \
         RETURNING {WHISKED_ORDER_COLS}"
    ))
    .bind(user_id)
    .bind(business_id)
    .bind(total_cents)
    .bind(pickup_code)
    .bind(stripe_payment_intent_id)
    .fetch_one(conn)
    .await
}

pub async fn get_by_id(
    conn:     &mut PgConnection,
    order_id: i32,
) -> AppResult<Option<WhiskedOrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {WHISKED_ORDER_COLS} FROM whisked_orders WHERE id = $1"
    ))
    .bind(order_id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

pub async fn list_active_for_business(
    conn:        &mut PgConnection,
    business_id: i32,
) -> AppResult<Vec<WhiskedOrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {WHISKED_ORDER_COLS} FROM whisked_orders \
         WHERE business_id = $1 AND status IN ('pending', 'preparing', 'ready') \
         ORDER BY created_at DESC"
    ))
    .bind(business_id)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}

/// Set `status`, returning the freshly-updated row. Used by the staff
/// dashboard `pending → preparing → ready` transitions; the `collected`
/// transition is a separate atomic operation that consumes the pickup code.
pub async fn update_status(
    conn:     &mut PgConnection,
    order_id: i32,
    status:   &str,
) -> AppResult<WhiskedOrderRow> {
    sqlx::query_as(&format!(
        "UPDATE whisked_orders \
         SET status = $2, updated_at = now() \
         WHERE id = $1 \
         RETURNING {WHISKED_ORDER_COLS}"
    ))
    .bind(order_id)
    .bind(status)
    .fetch_one(conn)
    .await
    .map_err(DomainError::Db)
}

/// Atomic pickup-code consumption. Marks the order `collected` and stamps
/// `pickup_code_used_at` only if the row currently matches `pickup_code`,
/// is still `ready`, and hasn't already been consumed. Returns the updated
/// row on success or `None` if the conditions weren't met (caller surfaces
/// the right user-facing error after pre-checking with `get_by_id`).
pub async fn collect_with_pickup_code(
    conn:        &mut PgConnection,
    order_id:    i32,
    pickup_code: &str,
) -> AppResult<Option<WhiskedOrderRow>> {
    sqlx::query_as(&format!(
        "UPDATE whisked_orders \
         SET status = 'collected', pickup_code_used_at = now(), updated_at = now() \
         WHERE id = $1 \
         AND pickup_code = $2 \
         AND pickup_code_used_at IS NULL \
         AND status = 'ready' \
         RETURNING {WHISKED_ORDER_COLS}"
    ))
    .bind(order_id)
    .bind(pickup_code)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)
}

/// Test-only helper: force a status without going through the staff-dashboard
/// path. Used by the validate_pickup tests to seed a `.ready` row.
#[cfg(test)]
pub async fn _test_force_status(
    conn:     &mut PgConnection,
    order_id: i32,
    status:   &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE whisked_orders SET status = $2, updated_at = now() WHERE id = $1")
        .bind(order_id)
        .bind(status)
        .execute(conn)
        .await
        .map(|_| ())
}

/// Verify that `business_user_id` is the registered owner of the business
/// associated with `order_id`. Returns the business id on success, or
/// `Forbidden`/`NotFound` if not.
pub async fn assert_business_owner(
    conn:             &mut PgConnection,
    order_id:         i32,
    business_user_id: i32,
) -> AppResult<i32> {
    let row: Option<(i32, i32)> = sqlx::query_as(
        "SELECT b.id, b.primary_holder_id \
         FROM whisked_orders o \
         JOIN businesses b ON b.id = o.business_id \
         WHERE o.id = $1"
    )
    .bind(order_id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)?;

    match row {
        None => Err(DomainError::NotFound),
        Some((biz_id, holder)) if holder == business_user_id => Ok(biz_id),
        Some(_) => Err(DomainError::Forbidden),
    }
}

pub async fn assert_business_owner_for_business(
    conn:             &mut PgConnection,
    business_id:      i32,
    business_user_id: i32,
) -> AppResult<()> {
    let holder: Option<i32> = sqlx::query_scalar(
        "SELECT primary_holder_id FROM businesses WHERE id = $1"
    )
    .bind(business_id)
    .fetch_optional(conn)
    .await
    .map_err(DomainError::Db)?;

    match holder {
        None                                     => Err(DomainError::NotFound),
        Some(uid) if uid == business_user_id     => Ok(()),
        Some(_)                                  => Err(DomainError::Forbidden),
    }
}

// ── Order items ──────────────────────────────────────────────────────────────

/// Insert one line item. Called inside the same transaction as the order.
pub async fn create_order_item(
    conn:         &mut PgConnection,
    order_id:     i32,
    menu_item_id: i32,
    quantity:     i32,
    price_cents:  i32,
) -> Result<WhiskedOrderItemRow, sqlx::Error> {
    sqlx::query_as(&format!(
        "INSERT INTO whisked_order_items \
         (order_id, menu_item_id, quantity, price_cents) \
         VALUES ($1, $2, $3, $4) \
         RETURNING {WHISKED_ORDER_ITEM_COLS}"
    ))
    .bind(order_id)
    .bind(menu_item_id)
    .bind(quantity)
    .bind(price_cents)
    .fetch_one(conn)
    .await
}

pub async fn list_items_for_order(
    conn:     &mut PgConnection,
    order_id: i32,
) -> AppResult<Vec<WhiskedOrderItemRow>> {
    sqlx::query_as(&format!(
        "SELECT {WHISKED_ORDER_ITEM_COLS} FROM whisked_order_items \
         WHERE order_id = $1 ORDER BY id ASC"
    ))
    .bind(order_id)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}

#[allow(dead_code)] // surface for future order-history endpoints
pub async fn list_orders_for_user(
    conn:    &mut PgConnection,
    user_id: i32,
) -> AppResult<Vec<WhiskedOrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {WHISKED_ORDER_COLS} FROM whisked_orders \
         WHERE user_id = $1 ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await
    .map_err(DomainError::Db)
}

#[allow(dead_code)] // helper for completeness / future ETA logic
pub async fn set_estimated_pickup(
    conn:     &mut PgConnection,
    order_id: i32,
    eta:      DateTime<Utc>,
) -> AppResult<()> {
    sqlx::query("UPDATE whisked_orders SET estimated_pickup_at = $2, updated_at = now() WHERE id = $1")
        .bind(order_id)
        .bind(eta)
        .execute(conn)
        .await
        .map(|_| ())
        .map_err(DomainError::Db)
}
