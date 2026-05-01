//! Handler-level integration tests covering failure modes not caught by SQL-layer tests.
//!
//! These tests exercise the full HTTP stack — router, middleware, extractors,
//! handlers — via `tower::ServiceExt::oneshot`. Each uses `sqlx::test` so it
//! runs against a fresh isolated database with all migrations applied.
//!
//! Run with:
//!   DATABASE_URL=postgres://localhost/test cargo test --test handler

mod common;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a Stripe webhook request with a valid signature for the test secret.
fn stripe_webhook_request(payload: serde_json::Value) -> Request<Body> {
    let body = serde_json::to_vec(&payload).unwrap();
    let sig  = common::sign_stripe_webhook(&body);
    Request::builder()
        .method("POST")
        .uri("/api/stripe/webhook")
        .header("content-type", "application/json")
        .header("stripe-signature", sig)
        .body(Body::from(body))
        .unwrap()
}

/// Build a Stripe webhook request with an invalid signature.
fn stripe_webhook_bad_sig(payload: serde_json::Value) -> Request<Body> {
    let body = serde_json::to_vec(&payload).unwrap();
    Request::builder()
        .method("POST")
        .uri("/api/stripe/webhook")
        .header("content-type", "application/json")
        .header("stripe-signature", "t=1,v1=deadbeef00000000000000000000000000000000000000000000000000000000")
        .body(Body::from(body))
        .unwrap()
}

/// A minimal payment_intent.succeeded Stripe event for a given type and pi_id.
fn stripe_event(pi_id: &str, payment_type: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "payment_intent.succeeded",
        "data": {
            "object": {
                "id": pi_id,
                "object": "payment_intent",
                "metadata": {
                    "type": payment_type
                }
            }
        }
    })
}

/// Build a Dorotka ask request, injecting ConnectInfo so the ConnectInfo
/// extractor doesn't fail. All requests use 127.0.0.1 so the rate limiter
/// groups them together.
fn dorotka_request(query: &str) -> Request<Body> {
    let body = serde_json::to_vec(&serde_json::json!({ "query": query })).unwrap();
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/dorotka/ask")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(body))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

// ─────────────────────────────────────────────────────────────────────────────
// Stripe webhook failures
// ─────────────────────────────────────────────────────────────────────────────

/// Valid signature + "membership" type but no pre-seeded pending row.
/// The handler must return 200 (Stripe always gets 200), leave the
/// memberships table empty, and write an audit_events miss record.
#[sqlx::test]
async fn stripe_webhook_membership_no_pending_row_returns_200_writes_audit(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let event = stripe_event("pi_test_membership_orphan", "membership");
    let resp  = app.oneshot(stripe_webhook_request(event)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK,
        "Stripe must always receive 200 even when no pending row exists");

    let membership_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(membership_count, 0,
        "no membership row must be created for an orphaned webhook");

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'payment.membership_not_found'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_count, 1,
        "audit_events must record the webhook miss so operators can investigate");
}

/// Valid signature + "order" type (the default/empty path) with an unknown pi_id.
/// No membership row must be written — verifies cross-type isolation.
#[sqlx::test]
async fn stripe_webhook_unknown_pi_returns_200_no_membership_written(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let event = stripe_event("pi_completely_unknown_xyz", "order");
    let resp  = app.oneshot(stripe_webhook_request(event)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK,
        "unknown pi_id must not cause a non-200 response");

    let membership_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(membership_count, 0,
        "an order-type webhook must never write to the memberships table");
}

/// A request with a forged Stripe-Signature must be rejected with 401.
/// No database writes must occur — the verification gate fires before any SQL.
#[sqlx::test]
async fn stripe_webhook_invalid_signature_returns_401_no_db_writes(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let event = stripe_event("pi_should_not_activate", "membership");
    let resp  = app.oneshot(stripe_webhook_bad_sig(event)).await.unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
        "forged signature must be rejected with 401 before any DB work");

    // Give any incorrectly fired async processing time to settle.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let membership_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memberships")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(membership_count, 0,
        "invalid signature must prevent all DB writes");
}

/// A request with no Stripe-Signature header at all must return 400.
#[sqlx::test]
async fn stripe_webhook_missing_sig_header_returns_400(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let body = serde_json::to_vec(&stripe_event("pi_no_header", "membership")).unwrap();
    let req  = Request::builder()
        .method("POST")
        .uri("/api/stripe/webhook")
        .header("content-type", "application/json")
        // Deliberately no stripe-signature header
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST,
        "missing Stripe-Signature header must return 400");
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth failures
// ─────────────────────────────────────────────────────────────────────────────

/// GET /api/auth/me without any Authorization header must return 401.
#[sqlx::test]
async fn auth_me_no_token_returns_401(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
        "missing Authorization header must return 401");
}

/// GET /api/auth/me with an expired JWT must return 401.
/// The token's signature is valid but exp is 1 (Unix epoch + 1 second).
#[sqlx::test]
async fn auth_me_expired_token_returns_401(pool: PgPool) {
    let user = common::create_user(&pool, "expired@test.com").await;
    let token = common::expired_token(i32::from(user.id));

    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED,
        "expired JWT must be rejected with 401");
}

/// GET /api/auth/me with a valid JWT for a banned user must return 403.
/// The JWT itself is valid and not expired — the ban check is in the handler.
#[sqlx::test]
async fn auth_me_banned_user_returns_403(pool: PgPool) {
    // Create a user and immediately ban them.
    let user = common::create_user(&pool, "banned@test.com").await;
    sqlx::query("UPDATE users SET banned = true WHERE id = $1")
        .bind(i32::from(user.id))
        .execute(&pool)
        .await
        .unwrap();

    let token = common::valid_token(i32::from(user.id));

    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN,
        "valid JWT for a banned user must return 403");
}

// ─────────────────────────────────────────────────────────────────────────────
// Rate limit behaviour
// ─────────────────────────────────────────────────────────────────────────────

/// 20 calls within the window → all pass the rate check (non-429).
/// The 21st call within the same 60-second window → 429.
///
/// Calls 1-20 will fail at the Anthropic API step (key not configured → 500),
/// which is expected and irrelevant — the rate check itself must pass for each.
#[sqlx::test]
async fn dorotka_rate_limit_21st_request_returns_429(pool: PgPool) {
    // Build the app once; the rate limiter is shared through the cloned AppState.
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    // Calls 1-20: rate check passes (they fail later at Anthropic key check → 500).
    for i in 1..=20u8 {
        let resp = app
            .clone()
            .oneshot(dorotka_request(&format!("query {i}")))
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "request {i} must not be rate-limited (limit is 20)"
        );
    }

    // Call 21: rate limiter has 20 entries in the window → deny.
    let resp = app
        .oneshot(dorotka_request("query 21"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "the 21st request within 60 seconds must be rate-limited");
}

/// Proves the rate limit is a sliding window, not a fixed block.
/// Uses a 3-req/1-s window for speed:
///   - 3 requests → all pass
///   - 4th immediately → 429 (window full)
///   - sleep 1.1 s (original 3 age out of the 1-second window)
///   - 5th → passes (window slid; no burst at boundary possible)
#[sqlx::test]
async fn dorotka_sliding_window_resets_after_window_elapses(pool: PgPool) {
    let state = common::build_state_with_dorotka_rate(pool.clone(), None, 3, 1);
    let app   = box_fraise_server::app::build(state);

    // Fill the 1-second, 3-request window.
    for i in 1..=3u8 {
        let resp = app
            .clone()
            .oneshot(dorotka_request(&format!("fill {i}")))
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
            "request {i} must pass while window has capacity");
    }

    // Window full — 4th is denied.
    let resp = app.clone().oneshot(dorotka_request("overflow")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "4th request must be rate-limited (window is at capacity)");

    // Wait for the 1-second window to slide past all 3 initial requests.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    // 5th request: the 3 original entries have aged out → window has capacity again.
    let resp = app.oneshot(dorotka_request("after window")).await.unwrap();
    assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "request after window elapses must not be rate-limited (sliding window reset)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Business scope enforcement
// ─────────────────────────────────────────────────────────────────────────────

/// A Cardputer device scoped to business A attempts to collect an order that
/// belongs to business B. The full RequireDevice extractor stack runs (EIP-191
/// signature verification + DB lookup) before the handler checks business scope.
/// Expected result: 403 Forbidden.
#[sqlx::test]
async fn device_collect_cross_business_returns_403(pool: PgPool) {
    // Business A — the device's business.
    let biz_a = common::create_business(&pool, "Biz A").await;
    let loc_a = common::create_location(&pool, biz_a.id, "Loc A").await;
    let _ = loc_a; // location_a exists for biz_a context; not needed for order

    // Business B — the order's business.
    let biz_b = common::create_business(&pool, "Biz B").await;
    let loc_b = common::create_location(&pool, biz_b.id, "Loc B").await;

    // Order belonging to biz_b.
    let nfc_token = common::create_ready_order(&pool, loc_b, biz_b.id).await;

    // Device scoped to biz_a.
    let signing_key = common::device_signing_key();
    let address     = common::device_eth_address(&signing_key);
    common::create_device(&pool, &address, "employee", Some(biz_a.id)).await;

    let auth_header = common::device_auth_header(&signing_key);

    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/orders/{nfc_token}/collect"))
        .header("authorization", auth_header)
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN,
        "device from Biz A must not collect an order belonging to Biz B");
}
