use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::{AppError, AppResult}, types::{OrderId, UserId}};
use super::types::OrderRow;

const ORDER_COLS: &str =
    "id, user_id, variety_id, location_id, time_slot_id, batch_id, quantity,
     chocolate, finish, is_gift, total_cents, stripe_payment_intent_id,
     status, nfc_token, rating, created_at";

// ── Reads ─────────────────────────────────────────────────────────────────────

pub async fn find_by_id(pool: &PgPool, id: OrderId) -> AppResult<Option<OrderRow>> {
    sqlx::query_as(&format!("SELECT {ORDER_COLS} FROM orders WHERE id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)
}

pub async fn list_for_user(pool: &PgPool, user_id: UserId) -> AppResult<Vec<OrderRow>> {
    sqlx::query_as(&format!(
        "SELECT {ORDER_COLS} FROM orders
         WHERE user_id = $1
         ORDER BY created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

// ── Stock guard ───────────────────────────────────────────────────────────────

/// Returns the current stock for a variety, locked for update.
pub async fn lock_variety_stock(
    tx:         &mut sqlx::Transaction<'_, sqlx::Postgres>,
    variety_id: i32,
) -> AppResult<i32> {
    let (stock,): (i32,) =
        sqlx::query_as("SELECT stock FROM varieties WHERE id = $1 FOR UPDATE")
            .bind(variety_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(AppError::Db)?;
    Ok(stock)
}

pub async fn decrement_stock(
    tx:         &mut sqlx::Transaction<'_, sqlx::Postgres>,
    variety_id: i32,
    qty:        i32,
) -> AppResult<()> {
    sqlx::query("UPDATE varieties SET stock = stock - $1 WHERE id = $2")
        .bind(qty)
        .bind(variety_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

// ── Create ────────────────────────────────────────────────────────────────────

pub async fn create(
    tx:           &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id:      Option<UserId>,
    variety_id:   i32,
    location_id:  i32,
    time_slot_id: Option<i32>,
    quantity:     i32,
    chocolate:    Option<&str>,
    finish:       Option<&str>,
    is_gift:      bool,
    total_cents:  i32,
    pi_id:        Option<&str>,
    status:       &str,
) -> AppResult<OrderRow> {
    let nfc_token = Uuid::new_v4().to_string();

    sqlx::query_as(&format!(
        "INSERT INTO orders
             (user_id, variety_id, location_id, time_slot_id, quantity,
              chocolate, finish, is_gift, total_cents, stripe_payment_intent_id,
              status, nfc_token)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
         RETURNING {ORDER_COLS}"
    ))
    .bind(user_id)
    .bind(variety_id)
    .bind(location_id)
    .bind(time_slot_id)
    .bind(quantity)
    .bind(chocolate)
    .bind(finish)
    .bind(is_gift)
    .bind(total_cents)
    .bind(pi_id)
    .bind(status)
    .bind(nfc_token)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::Db)
}

// ── Status transitions ────────────────────────────────────────────────────────

pub async fn set_status(pool: &PgPool, order_id: OrderId, status: &str) -> AppResult<()> {
    sqlx::query("UPDATE orders SET status = $1 WHERE id = $2")
        .bind(status)
        .bind(order_id)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

pub async fn set_rating(pool: &PgPool, order_id: OrderId, user_id: UserId, rating: i32) -> AppResult<()> {
    let rows = sqlx::query(
        "UPDATE orders SET rating = $1
         WHERE id = $2 AND user_id = $3 AND status = 'collected' AND rating IS NULL",
    )
    .bind(rating)
    .bind(order_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;

    if rows.rows_affected() == 0 {
        return Err(AppError::bad_request("order not found, not yours, not collected, or already rated"));
    }
    Ok(())
}

/// Atomically collect an order by NFC token. Returns the updated order.
pub async fn collect_by_nfc(
    pool:      &PgPool,
    nfc_token: &str,
    user_id:   Option<UserId>,
) -> AppResult<Option<OrderRow>> {
    // If user_id supplied, restrict to that user (self-collection).
    let row: Option<OrderRow> = if let Some(uid) = user_id {
        sqlx::query_as(&format!(
            "UPDATE orders SET status = 'collected'
             WHERE nfc_token = $1 AND user_id = $2 AND status = 'ready'
             RETURNING {ORDER_COLS}"
        ))
        .bind(nfc_token)
        .bind(uid)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)?
    } else {
        sqlx::query_as(&format!(
            "UPDATE orders SET status = 'collected'
             WHERE nfc_token = $1 AND status = 'ready'
             RETURNING {ORDER_COLS}"
        ))
        .bind(nfc_token)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Db)?
    };
    Ok(row)
}

// ── Time slot ─────────────────────────────────────────────────────────────────

pub async fn increment_slot_booked(
    tx:          &mut sqlx::Transaction<'_, sqlx::Postgres>,
    time_slot_id: i32,
) -> AppResult<()> {
    sqlx::query("UPDATE time_slots SET booked_count = booked_count + 1 WHERE id = $1")
        .bind(time_slot_id)
        .execute(&mut **tx)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

// ── Referral ──────────────────────────────────────────────────────────────────

/// Check if a referral code is valid and the user hasn't placed an order before.
pub async fn check_referral(
    pool:     &PgPool,
    user_id:  UserId,
    code:     &str,
) -> AppResult<bool> {
    // First order check.
    let has_orders: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM orders WHERE user_id = $1)")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::Db)?;

    if has_orders { return Ok(false); }

    // Valid referral code check.
    let valid: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM referral_codes WHERE code = $1 AND active = true)")
            .bind(code)
            .fetch_one(pool)
            .await
            .map_err(AppError::Db)?;

    Ok(valid)
}

// ── Balance payment ───────────────────────────────────────────────────────────

/// Atomically deduct from ad_balance_cents. Returns false if insufficient funds.
pub async fn deduct_balance(
    tx:          &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id:     UserId,
    amount_cents: i32,
) -> AppResult<bool> {
    let rows = sqlx::query(
        "UPDATE users
         SET ad_balance_cents = ad_balance_cents - $1
         WHERE id = $2 AND ad_balance_cents >= $1",
    )
    .bind(amount_cents)
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Db)?;
    Ok(rows.rows_affected() > 0)
}
