use crate::{
    app::AppState,
    error::{AppError, AppResult},
    types::{OrderId, StripeCustomerId, UserId},
};
use super::{
    repository,
    types::{CreateOrderBody, CreateOrderResponse, OrderRow, PaymentIntentResponse},
};

const REFERRAL_DISCOUNT: f64 = 0.10; // 10 %

// ── Create order ──────────────────────────────────────────────────────────────

pub async fn create_order(
    state:   &AppState,
    user_id: UserId,
    body:    CreateOrderBody,
) -> AppResult<CreateOrderResponse> {
    // Validate variety and compute price inside a transaction so the stock
    // check and decrement are atomic.
    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    let stock = repository::lock_variety_stock(&mut tx, body.variety_id).await?;
    if stock < body.quantity {
        return Err(AppError::bad_request("insufficient stock"));
    }

    let price: (i32,) = sqlx::query_as("SELECT price_cents FROM varieties WHERE id = $1")
        .bind(body.variety_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::Db)?;

    let total_cents = price.0 * body.quantity;

    // Create Stripe payment intent with manual capture.
    let user_row: Option<(Option<StripeCustomerId>,)> =
        sqlx::query_as("SELECT stripe_customer_id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::Db)?;

    let customer_id = user_row.and_then(|(c,)| c);

    let pi = state
        .stripe()
        .create_payment_intent(
            total_cents as i64,
            "cad",
            customer_id.as_ref().map(|c| c.as_str()),
            &[("order_user_id", &user_id.to_string())],
        )
        .await?;

    repository::decrement_stock(&mut tx, body.variety_id, body.quantity).await?;

    if let Some(slot_id) = body.time_slot_id {
        repository::increment_slot_booked(&mut tx, slot_id).await?;
    }

    let order = repository::create(
        &mut tx,
        Some(user_id),
        body.variety_id,
        body.location_id,
        body.time_slot_id,
        body.quantity,
        body.chocolate.as_deref(),
        body.finish.as_deref(),
        body.is_gift.unwrap_or(false),
        total_cents,
        Some(&pi.id),
        "pending",
    )
    .await?;

    tx.commit().await.map_err(AppError::Db)?;

    Ok(CreateOrderResponse {
        client_secret: pi.client_secret,
        order,
    })
}

// ── Confirm payment ───────────────────────────────────────────────────────────

pub async fn confirm_order(state: &AppState, order_id: OrderId, user_id: UserId) -> AppResult<OrderRow> {
    let order = repository::find_by_id(&state.db, order_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if order.user_id != Some(user_id) {
        return Err(AppError::Forbidden);
    }

    if order.status != "pending" {
        return Err(AppError::bad_request("order is not pending"));
    }

    // Verify Stripe PI was paid.
    if let Some(ref pi_id) = order.stripe_payment_intent_id {
        let pi = state.stripe().get_payment_intent(pi_id).await?;
        if pi.status != "requires_capture" && pi.status != "succeeded" {
            return Err(AppError::bad_request("payment not confirmed"));
        }
    }

    repository::set_status(&state.db, order_id, "queued").await?;

    // TODO: check batch threshold and trigger if met
    // TODO: send queued/triggered email via integrations::resend
    // TODO: add legitimacy event

    repository::find_by_id(&state.db, order_id)
        .await?
        .ok_or(AppError::NotFound)
}

// ── Payment intent (without creating order) ───────────────────────────────────

pub async fn create_payment_intent(
    state:        &AppState,
    user_id:      UserId,
    variety_id:   i32,
    quantity:     i32,
    referral_code: Option<&str>,
) -> AppResult<PaymentIntentResponse> {
    let (stock, price): (i32, i32) = sqlx::query_as(
        "SELECT stock, price_cents FROM varieties WHERE id = $1 AND active = true",
    )
    .bind(variety_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .ok_or(AppError::NotFound)?;

    if stock < quantity {
        return Err(AppError::bad_request("insufficient stock"));
    }

    let mut total_cents = price * quantity;
    let mut discount_applied = false;

    if let Some(code) = referral_code {
        if repository::check_referral(&state.db, user_id, code).await? {
            total_cents = (total_cents as f64 * (1.0 - REFERRAL_DISCOUNT)) as i32;
            discount_applied = true;
        }
    }

    let pi = state
        .stripe()
        .create_payment_intent(total_cents as i64, "cad", None, &[])
        .await?;

    Ok(PaymentIntentResponse {
        client_secret: pi.client_secret.unwrap_or_default(),
        total_cents,
        discount_applied,
    })
}

// ── Pay with ad_balance_cents ─────────────────────────────────────────────────

pub async fn pay_with_balance(
    state:   &AppState,
    user_id: UserId,
    body:    super::types::CreateOrderBody,
) -> AppResult<OrderRow> {
    let mut tx = state.db.begin().await.map_err(AppError::Db)?;

    let stock = repository::lock_variety_stock(&mut tx, body.variety_id).await?;
    if stock < body.quantity {
        return Err(AppError::bad_request("insufficient stock"));
    }

    let (price,): (i32,) = sqlx::query_as("SELECT price_cents FROM varieties WHERE id = $1")
        .bind(body.variety_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::Db)?;

    let total_cents = price * body.quantity;

    let ok = repository::deduct_balance(&mut tx, user_id, total_cents).await?;
    if !ok {
        return Err(AppError::bad_request("insufficient balance"));
    }

    repository::decrement_stock(&mut tx, body.variety_id, body.quantity).await?;

    let order = repository::create(
        &mut tx,
        Some(user_id),
        body.variety_id,
        body.location_id,
        body.time_slot_id,
        body.quantity,
        body.chocolate.as_deref(),
        body.finish.as_deref(),
        body.is_gift.unwrap_or(false),
        total_cents,
        None,
        "queued",
    )
    .await?;

    tx.commit().await.map_err(AppError::Db)?;
    Ok(order)
}
