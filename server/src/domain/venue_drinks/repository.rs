use sqlx::PgPool;

use crate::{error::{AppError, AppResult}, types::UserId};
use super::types::{DrinkRow, VenueOrderRow};

// ── Menu ──────────────────────────────────────────────────────────────────────

pub async fn get_menu(pool: &PgPool, business_id: i32) -> AppResult<Vec<DrinkRow>> {
    sqlx::query_as(
        "SELECT id, name, description, price_cents, category, sort_order
         FROM venue_drinks
         WHERE business_id = $1 AND available = true
         ORDER BY sort_order ASC, id ASC"
    )
    .bind(business_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn get_drink(pool: &PgPool, drink_id: i64, business_id: i32) -> AppResult<Option<DrinkRow>> {
    sqlx::query_as(
        "SELECT id, name, description, price_cents, category, sort_order
         FROM venue_drinks
         WHERE id = $1 AND business_id = $2 AND available = true"
    )
    .bind(drink_id)
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

// ── Business Connect account ──────────────────────────────────────────────────

pub async fn get_connect_account(
    pool:        &PgPool,
    business_id: i32,
) -> AppResult<Option<String>> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT stripe_connect_account_id FROM businesses WHERE id = $1"
    )
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.and_then(|(acct,)| acct))
}

pub async fn set_connect_account(
    pool:        &PgPool,
    business_id: i32,
    account_id:  &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE businesses SET stripe_connect_account_id = $2 WHERE id = $1"
    )
    .bind(business_id)
    .bind(account_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

// ── Orders ────────────────────────────────────────────────────────────────────

pub struct NewOrder<'a> {
    pub user_id:          UserId,
    pub business_id:      i32,
    pub idempotency_key:  &'a str,
    pub pi_id:            &'a str,
    pub total_cents:      i32,
    pub platform_fee_cents: i32,
    pub notes:            &'a str,
}

pub async fn insert_order(pool: &PgPool, o: NewOrder<'_>) -> AppResult<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO venue_orders
             (user_id, business_id, idempotency_key, stripe_payment_intent_id,
              total_cents, platform_fee_cents, notes)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id"
    )
    .bind(o.user_id)
    .bind(o.business_id)
    .bind(o.idempotency_key)
    .bind(o.pi_id)
    .bind(o.total_cents)
    .bind(o.platform_fee_cents)
    .bind(o.notes)
    .fetch_one(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(id)
}

pub async fn insert_order_item(
    pool:       &PgPool,
    order_id:   i64,
    drink_id:   i64,
    drink_name: &str,
    price_cents: i32,
    quantity:   i32,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO venue_order_items (order_id, drink_id, drink_name, price_cents, quantity)
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(order_id)
    .bind(drink_id)
    .bind(drink_name)
    .bind(price_cents)
    .bind(quantity)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn get_order_by_pi(
    pool:  &PgPool,
    pi_id: &str,
) -> AppResult<Option<VenueOrderRow>> {
    sqlx::query_as(
        "SELECT id, user_id, business_id, stripe_payment_intent_id, square_order_id,
                status, total_cents, platform_fee_cents, notes, created_at
         FROM venue_orders
         WHERE stripe_payment_intent_id = $1"
    )
    .bind(pi_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)
}

pub async fn get_order_by_idempotency_key(
    pool:            &PgPool,
    idempotency_key: &str,
    pi_id:           &str,
) -> AppResult<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM venue_orders
         WHERE idempotency_key = $1 AND stripe_payment_intent_id = $2"
    )
    .bind(idempotency_key)
    .bind(pi_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(row.map(|(id,)| id))
}

pub async fn update_order_status(
    pool:     &PgPool,
    order_id: i64,
    status:   &str,
) -> AppResult<()> {
    sqlx::query("UPDATE venue_orders SET status = $2 WHERE id = $1")
        .bind(order_id)
        .bind(status)
        .execute(pool)
        .await
        .map_err(AppError::Db)?;
    Ok(())
}

pub async fn set_square_order_id(
    pool:            &PgPool,
    order_id:        i64,
    square_order_id: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE venue_orders SET square_order_id = $2, status = 'pushed_to_square' WHERE id = $1"
    )
    .bind(order_id)
    .bind(square_order_id)
    .execute(pool)
    .await
    .map_err(AppError::Db)?;
    Ok(())
}

pub async fn get_order_items_for_square(
    pool:     &PgPool,
    order_id: i64,
) -> AppResult<Vec<(String, i32, i32)>> {
    // Returns (drink_name, price_cents, quantity)
    let rows: Vec<(String, i32, i32)> = sqlx::query_as(
        "SELECT drink_name, price_cents, quantity
         FROM venue_order_items
         WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::Db)?;

    Ok(rows)
}
