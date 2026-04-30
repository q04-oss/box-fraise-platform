//! Integration tests for Square order webhook signature verification.
//!
//! These tests exercise the full HTTP path through the Axum router so that
//! the verification gate is tested at the layer where it actually runs.
//!
//! Run with:
//!   DATABASE_URL=postgres://localhost/test cargo test --test square_webhook

mod common;

use axum::{body::Body, http::{Request, StatusCode}};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use common::{SQUARE_NOTIFICATION_URL, SQUARE_SIGNING_KEY};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Builds an `order.updated` COMPLETED payload for a given square_order_id.
fn order_completed_payload(square_order_id: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "type": "order.updated",
        "event_id": Uuid::new_v4().to_string(),
        "data": {
            "type": "order.updated",
            "object": {
                "order_updated": {
                    "state":       "COMPLETED",
                    "order_id":    square_order_id,
                    "version":     2,
                    "location_id": "test-location"
                }
            }
        }
    }))
    .unwrap()
}

/// Inserts a venue order already in the `pushed_to_square` state.
/// Returns the order's database id.
async fn insert_pushed_order(pool: &PgPool, user_id: i32, business_id: i32, square_order_id: &str) -> i64 {
    let idem   = Uuid::new_v4().to_string();
    let pi_id  = format!("pi_test_{}", Uuid::new_v4().simple());
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO venue_orders
             (user_id, business_id, idempotency_key, stripe_payment_intent_id,
              square_order_id, status, total_cents)
         VALUES ($1, $2, $3, $4, $5, 'pushed_to_square', 500)
         RETURNING id"
    )
    .bind(user_id)
    .bind(business_id)
    .bind(idem)
    .bind(pi_id)
    .bind(square_order_id)
    .fetch_one(pool)
    .await
    .expect("insert_pushed_order");
    id
}

// ── Test 1: valid signature → 200, payload processed ─────────────────────────

/// A correctly signed webhook must pass the verification gate (200) and
/// trigger order completion. After the handler returns, the venue_order
/// status must change to 'completed' and a loyalty steep must be recorded.
#[sqlx::test(migrations = "migrations")]
async fn valid_signature_returns_200_and_processes_payload(pool: PgPool) {
    let state = common::build_state_with_square(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let square_order_id = "sq-order-valid-sig-test";
    let order_id = insert_pushed_order(
        &pool, i32::from(customer.id), biz.id, square_order_id
    ).await;

    let body = order_completed_payload(square_order_id);
    let sig  = common::sign_square_payload(SQUARE_SIGNING_KEY, SQUARE_NOTIFICATION_URL, &body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/square/orders")
                .header("content-type", "application/json")
                .header("x-square-hmacsha256-signature", sig)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK,
        "valid signature must return 200");

    // Wait for the spawned processing task to complete.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM venue_orders WHERE id = $1"
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "completed",
        "order must be marked completed after valid webhook");

    let steep_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events
         WHERE user_id = $1 AND event_type = 'steep_earned'"
    )
    .bind(i32::from(customer.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(steep_count, 1,
        "loyalty steep must be credited after drink collection");
}

// ── Test 2: invalid signature → 401, payload not processed ───────────────────

/// A request with an incorrect signature must be rejected before any database
/// work begins. The venue_order status must remain unchanged.
#[sqlx::test(migrations = "migrations")]
async fn invalid_signature_returns_401_and_does_not_process_payload(pool: PgPool) {
    let state = common::build_state_with_square(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let square_order_id = "sq-order-invalid-sig-test";
    let order_id = insert_pushed_order(
        &pool, i32::from(customer.id), biz.id, square_order_id
    ).await;

    let body = order_completed_payload(square_order_id);
    // Sign with the wrong key — signature won't match.
    let bad_sig = common::sign_square_payload("wrong-key-entirely", SQUARE_NOTIFICATION_URL, &body);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/square/orders")
                .header("content-type", "application/json")
                .header("x-square-hmacsha256-signature", bad_sig)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED,
        "invalid signature must return 401");

    // Give time for any (incorrectly fired) processing to complete.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM venue_orders WHERE id = $1"
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pushed_to_square",
        "order status must be unchanged after rejected webhook");
}

// ── Test 3: missing signature header → 401 ───────────────────────────────────

/// A request with no `x-square-hmacsha256-signature` header must be rejected
/// immediately. No database work should begin.
#[sqlx::test(migrations = "migrations")]
async fn missing_signature_header_returns_401(pool: PgPool) {
    let state = common::build_state_with_square(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let body = order_completed_payload("sq-order-no-header");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/square/orders")
                .header("content-type", "application/json")
                // Deliberately no x-square-hmacsha256-signature header
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED,
        "missing signature header must return 401");
}
