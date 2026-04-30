use secrecy::ExposeSecret;

use crate::{
    audit,
    app::AppState,
    domain::{loyalty, squareoauth},
    error::{AppError, AppResult},
    integrations::square::ApiClient,
    integrations::square::OrderLineItem,
    types::UserId,
};
use super::{
    repository::{self, NewOrder},
    types::{ConnectOnboardingResponse, CreateVenueOrderBody, DrinkRow, VenueOrderResponse},
};

// ── Menu ──────────────────────────────────────────────────────────────────────

pub async fn get_menu(state: &AppState, business_id: i32) -> AppResult<Vec<DrinkRow>> {
    repository::get_menu(&state.db, business_id).await
}

// ── Create order ──────────────────────────────────────────────────────────────

/// Creates a Stripe PaymentIntent for a venue drink order and records the
/// pending order in the database.
///
/// Security invariants enforced here:
///   • Prices are read from the DB — the client sends only drink_id + quantity.
///   • The platform fee is computed from PLATFORM_FEE_BIPS in config.
///   • The Stripe Connect account is read from the businesses row.
///   • The idempotency_key UNIQUE constraint means retried POSTs return the
///     existing order rather than creating duplicates.
pub async fn create_order(
    state:   &AppState,
    user_id: UserId,
    body:    CreateVenueOrderBody,
    ip:      Option<std::net::IpAddr>,
) -> AppResult<VenueOrderResponse> {
    if body.items.is_empty() {
        return Err(AppError::bad_request("order must contain at least one item"));
    }

    // ── Validate idempotency key ──────────────────────────────────────────────
    let idem_key = body.idempotency_key.trim();
    if idem_key.len() < 8 || idem_key.len() > 128 {
        return Err(AppError::bad_request("idempotency_key must be 8–128 characters"));
    }

    // ── Resolve Connect account ───────────────────────────────────────────────
    let connect_account = repository::get_connect_account(&state.db, body.business_id)
        .await?
        .ok_or_else(|| AppError::bad_request(
            "this business has not set up online payments yet"
        ))?;

    // ── Validate items and compute total server-side ──────────────────────────
    let mut total_cents: i64 = 0;
    let mut line_items: Vec<(DrinkRow, i32)> = Vec::with_capacity(body.items.len());

    for item in &body.items {
        if item.quantity < 1 || item.quantity > 20 {
            return Err(AppError::bad_request("quantity must be between 1 and 20"));
        }
        let drink = repository::get_drink(&state.db, item.drink_id, body.business_id)
            .await?
            .ok_or_else(|| AppError::bad_request(
                &format!("drink {} is not available at this location", item.drink_id)
            ))?;

        total_cents += (drink.price_cents as i64) * (item.quantity as i64);
        line_items.push((drink, item.quantity));
    }

    if total_cents > 50_000 {
        return Err(AppError::bad_request("order total exceeds maximum"));
    }

    // ── Compute platform fee ──────────────────────────────────────────────────
    let fee_cents = (total_cents * state.cfg.platform_fee_bips) / 10_000;

    // ── Resolve Stripe customer ───────────────────────────────────────────────
    let stripe_customer: Option<String> = sqlx::query_scalar(
        "SELECT stripe_customer_id FROM users WHERE id = $1"
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .flatten();

    // ── Create Stripe PaymentIntent ───────────────────────────────────────────
    let pi = state.stripe().create_payment_intent_connect(
        total_cents,
        fee_cents,
        &connect_account,
        stripe_customer.as_deref(),
        &[
            ("type",        "venue_order"),
            ("business_id", &body.business_id.to_string()),
            ("user_id",     &i32::from(user_id).to_string()),
        ],
    ).await?;

    let client_secret = pi.client_secret
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("PaymentIntent missing client_secret")))?;

    // ── Insert order + items ──────────────────────────────────────────────────
    let order_id = repository::insert_order(&state.db, NewOrder {
        user_id,
        business_id:       body.business_id,
        idempotency_key:   idem_key,
        pi_id:             &pi.id,
        total_cents:       total_cents as i32,
        platform_fee_cents: fee_cents as i32,
        notes:             body.notes.as_deref().unwrap_or(""),
    }).await
    .map_err(|e| match e {
        AppError::Db(sqlx::Error::Database(ref db)) if db.is_unique_violation() => {
            // Idempotency key already used — return a conflict so the caller
            // can fetch the existing order instead.
            AppError::Conflict("an order with this idempotency_key already exists".into())
        }
        other => other,
    })?;

    for (drink, qty) in &line_items {
        repository::insert_order_item(
            &state.db, order_id, drink.id, &drink.name, drink.price_cents, *qty
        ).await?;
    }

    audit::write(
        &state.db,
        Some(user_id.into()),
        Some(body.business_id),
        "venue_order.payment_intent_created",
        serde_json::json!({
            "order_id":          order_id,
            "stripe_pi_id":      pi.id,
            "total_cents":       total_cents,
            "platform_fee_cents": fee_cents,
        }),
        ip,
    ).await;

    Ok(VenueOrderResponse {
        order_id,
        client_secret,
        total_cents: total_cents as i32,
    })
}

// ── Webhook handler (called by payments::routes on payment_intent.succeeded) ──

/// Completes a venue order after Stripe confirms payment:
///   1. Mark order as paid
///   2. Push to Square POS
///   3. Credit loyalty steep
///   4. Audit all three steps
///
/// Designed for at-least-once delivery — every step is idempotent:
///   • status update uses conditional WHERE status = 'paid' or 'pending'
///   • Square push idempotency_key = pi_id (Square rejects duplicates)
///   • loyalty insert has UNIQUE(idempotency_key) — pi_id used as key
pub async fn complete_venue_order(state: &AppState, pi_id: &str) {
    let result = complete_venue_order_inner(state, pi_id).await;

    if let Err(e) = result {
        tracing::error!(
            pi_id,
            error = %e,
            "venue_order completion failed — manual intervention may be required"
        );
        // Best-effort: mark failed so the dashboard can surface it.
        if let Ok(Some(order)) = repository::get_order_by_pi(&state.db, pi_id).await {
            if order.status == "paid" {
                // Square push failed after payment — mark failed, don't refund automatically.
                let _ = repository::update_order_status(&state.db, order.id, "failed").await;
                audit::write(
                    &state.db,
                    None,
                    Some(order.business_id),
                    "venue_order.square_push_failed",
                    serde_json::json!({
                        "order_id": order.id,
                        "pi_id":    pi_id,
                        "error":    e.to_string(),
                    }),
                    None,
                ).await;
            }
        }
    }
}

async fn complete_venue_order_inner(state: &AppState, pi_id: &str) -> AppResult<()> {
    // ── 1. Mark paid (idempotent: WHERE status = 'pending') ──────────────────
    let order = repository::get_order_by_pi(&state.db, pi_id)
        .await?
        .ok_or_else(|| AppError::NotFound)?;

    if order.status != "pending" {
        // Already processed — webhook retry or race condition.
        tracing::info!(pi_id, status = order.status, "venue_order already past pending — skipping");
        return Ok(());
    }

    repository::update_order_status(&state.db, order.id, "paid").await?;

    audit::write(
        &state.db,
        None,
        Some(order.business_id),
        "venue_order.payment_confirmed",
        serde_json::json!({ "order_id": order.id, "pi_id": pi_id }),
        None,
    ).await;

    // ── 2. Push to Square POS ─────────────────────────────────────────────────
    let tokens = squareoauth::service::load_decrypted(state, order.business_id).await?;

    let items = repository::get_order_items_for_square(&state.db, order.id).await?;
    let line_items: Vec<OrderLineItem> = items.iter().map(|(name, price, qty)| OrderLineItem {
        name:        name.clone(),
        quantity:    qty.to_string(),
        price_cents: *price as i64,
    }).collect();

    let square_client = ApiClient::new(&tokens.access_token, &state.http);
    let square_order_id = square_client.create_order(
        &tokens.square_location_id,
        &line_items,
        &order.id.to_string(),
        pi_id, // idempotency key — Square rejects duplicate pi_ids
    ).await?;

    repository::set_square_order_id(&state.db, order.id, &square_order_id).await?;

    audit::write(
        &state.db,
        None,
        Some(order.business_id),
        "venue_order.square_push_succeeded",
        serde_json::json!({
            "order_id":        order.id,
            "square_order_id": square_order_id,
        }),
        None,
    ).await;

    // Steep is credited when the drink is collected (Square order.updated →
    // COMPLETED), not at payment time. See complete_order_from_square below.

    Ok(())
}

// ── Square order.updated → COMPLETED ─────────────────────────────────────────

/// Called when Square fires order.updated with state == COMPLETED.
/// This is the moment the drink has been handed to the customer — the correct
/// trigger for crediting the loyalty steep.
///
/// Idempotency: square_order_id is used as the loyalty idempotency key.
/// If Square retries the webhook the loyalty UNIQUE constraint rejects the
/// duplicate without returning an error.
pub async fn complete_order_from_square(state: &AppState, square_order_id: &str) {
    if let Err(e) = complete_order_from_square_inner(state, square_order_id).await {
        tracing::error!(
            square_order_id,
            error = %e,
            "complete_order_from_square failed"
        );
    }
}

async fn complete_order_from_square_inner(
    state:           &AppState,
    square_order_id: &str,
) -> AppResult<()> {
    let order = match repository::get_order_by_square_id(&state.db, square_order_id).await? {
        Some(o) => o,
        None    => {
            // Order not in our system — could be a walk-in Square order, not an app order.
            tracing::debug!(square_order_id, "order.updated for unknown Square order — ignoring");
            return Ok(());
        }
    };

    // Guard: only process orders that reached the POS successfully.
    if order.status != "pushed_to_square" {
        tracing::debug!(square_order_id, status = order.status, "order not in pushed_to_square state — skipping");
        return Ok(());
    }

    // Mark completed.
    repository::update_order_status(&state.db, order.id, "completed").await?;

    let user_id = UserId::from(order.user_id);

    // Credit the loyalty steep now that the drink has been collected.
    match loyalty::service::record_steep_from_webhook(
        state, user_id, order.business_id, square_order_id,
    ).await {
        Ok(()) | Err(AppError::Conflict(_)) => {}
        Err(e) => tracing::error!(square_order_id, error = %e, "loyalty steep failed"),
    }

    audit::write(
        &state.db,
        None,
        Some(order.business_id),
        "venue_order.completed",
        serde_json::json!({
            "order_id":        order.id,
            "square_order_id": square_order_id,
        }),
        None,
    ).await;

    // Push notification to customer.
    if let Ok(Some(push_token)) = repository::get_user_push_token(&state.db, order.user_id).await {
        let business_name = repository::get_business_name(&state.db, order.business_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| "the café".to_string());

        let _ = crate::integrations::expo_push::send(
            &state.http,
            crate::integrations::expo_push::PushMessage {
                to:    &push_token,
                title: Some("steep earned"),
                body:  &format!("enjoy your drink from {business_name}"),
                data:  Some(serde_json::json!({
                    "type":        "steep_earned",
                    "business_id": order.business_id,
                })),
                sound: "default",
            },
        ).await;
    }

    Ok(())
}

// ── Stripe Connect onboarding ─────────────────────────────────────────────────

pub async fn onboard_stripe_connect(
    state:       &AppState,
    staff_uid:   UserId,
    business_id: i32,
    ip:          Option<std::net::IpAddr>,
) -> AppResult<ConnectOnboardingResponse> {
    // Fetch business email for the Connect account.
    let email: Option<String> = sqlx::query_scalar(
        "SELECT email FROM users
         WHERE id = (SELECT owner_id FROM businesses WHERE id = $1 LIMIT 1)"
    )
    .bind(business_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Db)?
    .flatten();

    let email = email.unwrap_or_else(|| format!("business-{}@boxfraise.com", business_id));

    // Create or reuse Connect account.
    let account_id = match repository::get_connect_account(&state.db, business_id).await? {
        Some(id) => id,
        None => {
            let id = state.stripe().create_connect_account(&email).await?;
            repository::set_connect_account(&state.db, business_id, &id).await?;
            id
        }
    };

    // Derive base URL from the configured redirect URL as a safe default.
    let base = state.cfg.square_oauth_redirect_url
        .as_deref()
        .and_then(|u| u.split("/api/").next())
        .unwrap_or("https://boxfraise.com");

    let url = state.stripe().create_account_link(
        &account_id,
        &format!("{base}/stripe-connect/refresh"),
        &format!("{base}/stripe-connect/return"),
    ).await?;

    audit::write(
        &state.db,
        Some(staff_uid.into()),
        Some(business_id),
        "venue_order.stripe_connect_onboarding_started",
        serde_json::json!({ "connect_account_id": account_id }),
        ip,
    ).await;

    Ok(ConnectOnboardingResponse { onboarding_url: url })
}
