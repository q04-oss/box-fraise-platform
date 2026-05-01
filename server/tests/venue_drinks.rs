//! Integration tests for the venue drinks service.
//!
//! Note on Stripe: create_order calls Stripe's API to create a PaymentIntent.
//! Tests that need to assert on a successfully created order (e.g., confirming
//! prices come from DB rather than from the client) require either HTTP mocking
//! or a live Stripe test key — neither is in place yet. Those tests are marked
//! with a comment and left as stubs until an HTTP mock layer is added.
//!
//! What IS tested here:
//!   - Menu returns only available drinks (pure DB, no Stripe)
//!   - create_order early-exit paths that occur before any Stripe call
//!   - Prices-from-DB is structurally enforced: the CreateVenueOrderBody type
//!     carries no price field — the server resolves price from the drinks table
//!
//! Run with:
//!   DATABASE_URL=postgres://localhost/test cargo test --test venue_drinks

mod common;

use box_fraise_server::{
    domain::venue_drinks::{service as venue, types::{CreateVenueOrderBody, OrderItem}},
    error::AppError,
};
use sqlx::PgPool;

// ── Stripe webhook idempotency ────────────────────────────────────────────────

/// The `if order.status != "pending"` guard in complete_venue_order_inner is the
/// entire safety net against double-processing when Stripe retries the webhook.
/// A customer would be charged once but their order pushed to Square twice —
/// or their loyalty steep credited twice — if this guard were absent or broken.
///
/// This test seeds a venue_order already in 'paid' status and asserts that a
/// second complete_venue_order call is a strict no-op: no audit event written,
/// no loyalty event credited, order status unchanged.
#[sqlx::test]
async fn complete_venue_order_is_idempotent_after_paid(pool: PgPool) {
    let state    = common::build_state(pool.clone(), None);
    let customer = common::create_user(&pool, "customer@idempotency.test").await;
    let biz      = common::create_business(&pool, "Idempotency Café").await;
    let pi_id    = "pi_idempotency_test_00000001";

    // Seed an order that has already been processed (status = 'paid').
    common::seed_paid_venue_order(&pool, biz.id, customer.id, pi_id).await;

    // Calling complete_venue_order on an already-paid order must be a no-op.
    // It should return normally — not panic, not error — and change nothing.
    venue::complete_venue_order(&state, pi_id).await;

    // ── Assert: order status unchanged ────────────────────────────────────────
    let status: String = sqlx::query_scalar(
        "SELECT status FROM venue_orders WHERE stripe_payment_intent_id = $1"
    )
    .bind(pi_id)
    .fetch_one(&pool)
    .await
    .expect("order must still exist");

    assert_eq!(status, "paid", "idempotency guard must leave status unchanged");

    // ── Assert: no payment_confirmed audit event written ──────────────────────
    // The guard fires before the audit — if payment_confirmed appears, the guard
    // was bypassed and the order was reprocessed.
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events
         WHERE event_type = 'venue_order.payment_confirmed'
           AND metadata->>'pi_id' = $1"
    )
    .bind(pi_id)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

    assert_eq!(audit_count, 0, "no payment_confirmed audit must be written for an already-paid order");

    // ── Assert: no loyalty event credited ────────────────────────────────────
    let loyalty_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events
         WHERE user_id = $1 AND business_id = $2"
    )
    .bind(i32::from(customer.id))
    .bind(biz.id)
    .fetch_one(&pool)
    .await
    .unwrap_or(0);

    assert_eq!(loyalty_count, 0, "no loyalty steep must be credited when order was already paid");
}

// ── Fixture helpers ───────────────────────────────────────────────────────────

async fn insert_drink(
    pool:        &sqlx::PgPool,
    business_id: i32,
    name:        &str,
    price_cents: i32,
    available:   bool,
) -> i64 {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO venue_drinks (business_id, name, price_cents, available)
         VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind(business_id)
    .bind(name)
    .bind(price_cents)
    .bind(available)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("insert_drink: {e}"));
    id
}

// ── Menu: only available drinks returned ─────────────────────────────────────

/// get_menu must filter out unavailable drinks. Only drinks with available = true
/// should appear on the menu.
#[sqlx::test]
async fn get_menu_excludes_unavailable_drinks(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let biz   = common::create_business(&pool, "Test Café").await;

    insert_drink(&pool, biz.id, "Matcha Latte",   550, true).await;
    insert_drink(&pool, biz.id, "Seasonal Special", 650, false).await; // unavailable
    insert_drink(&pool, biz.id, "Espresso",        300, true).await;

    let menu = venue::get_menu(&state, biz.id)
        .await
        .expect("get_menu must succeed");

    assert_eq!(menu.len(), 2, "menu must contain only available drinks");
    let names: Vec<&str> = menu.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&"Matcha Latte"), "available drink must be in menu");
    assert!(names.contains(&"Espresso"),     "available drink must be in menu");
    assert!(!names.contains(&"Seasonal Special"), "unavailable drink must be absent");
}

/// Menu for a business with no drinks must return an empty list, not an error.
#[sqlx::test]
async fn get_menu_empty_for_new_business(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let biz   = common::create_business(&pool, "New Café").await;

    let menu = venue::get_menu(&state, biz.id).await.expect("get_menu must succeed");
    assert!(menu.is_empty(), "new business must have an empty menu");
}

// ── create_order: early-exit validation ──────────────────────────────────────

/// A business without a Stripe Connect account must return a meaningful error
/// before any Stripe API call is made. This confirms the Connect account guard
/// fires and the error message is user-facing (not an internal 500).
#[sqlx::test]
async fn create_order_fails_without_stripe_connect_account(pool: PgPool) {
    let state    = common::build_state(pool.clone(), None);
    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Not Connected Café").await;
    // No stripe_connect_account_id set on this business.

    let body = CreateVenueOrderBody {
        business_id:     biz.id,
        idempotency_key: "idem-no-connect-00000000".to_string(),
        items:           vec![OrderItem { drink_id: 1, quantity: 1 }],
        notes:           None,
    };

    let result = venue::create_order(&state, customer.id, body, None).await;
    assert!(
        matches!(result, Err(AppError::BadRequest(_))),
        "missing Connect account must return BadRequest, got: {result:?}"
    );
}

/// An empty items list must be rejected before reaching any external call.
#[sqlx::test]
async fn create_order_rejects_empty_items(pool: PgPool) {
    let state    = common::build_state(pool.clone(), None);
    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;

    let body = CreateVenueOrderBody {
        business_id:     biz.id,
        idempotency_key: "idem-empty-items-00000000".to_string(),
        items:           vec![],
        notes:           None,
    };

    let result = venue::create_order(&state, customer.id, body, None).await;
    assert!(
        matches!(result, Err(AppError::BadRequest(_))),
        "empty items must return BadRequest, got: {result:?}"
    );
}

/// A drink_id that belongs to a different business (or doesn't exist) must be
/// rejected. This guards against cross-business order injection.
#[sqlx::test]
async fn create_order_rejects_drink_from_wrong_business(pool: PgPool) {
    let state    = common::build_state(pool.clone(), None);
    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz1     = common::create_business(&pool, "Biz One").await;
    let biz2     = common::create_business(&pool, "Biz Two").await;

    // Insert a drink on biz2, then try to order it via biz1.
    let biz2_drink = insert_drink(&pool, biz2.id, "Oat Matcha", 550, true).await;

    // Set a Connect account on biz1 so the order passes the early guard.
    sqlx::query(
        "UPDATE businesses SET stripe_connect_account_id = 'acct_test' WHERE id = $1"
    )
    .bind(biz1.id)
    .execute(&pool)
    .await
    .unwrap();

    let body = CreateVenueOrderBody {
        business_id:     biz1.id,
        idempotency_key: "idem-wrong-biz-00000000".to_string(),
        items:           vec![OrderItem { drink_id: biz2_drink, quantity: 1 }],
        notes:           None,
    };

    let result = venue::create_order(&state, customer.id, body, None).await;
    assert!(
        matches!(result, Err(AppError::BadRequest(_))),
        "drink from wrong business must return BadRequest, got: {result:?}"
    );
}

// ── Price-from-DB: structural note ───────────────────────────────────────────
//
// The full "prices come from DB" assertion — verifying that total_cents in the
// created order matches DB price × quantity, not any client-supplied value —
// requires a successfully created order, which requires a live Stripe call.
//
// The structural guarantee is already enforced: CreateVenueOrderBody contains
// no price field. The server resolves price exclusively from the venue_drinks
// table before constructing the PaymentIntent.
//
// The integration test for this lives in tests/venue_drinks_stripe.rs (not yet
// written — pending HTTP mock setup or Stripe test-key configuration).
