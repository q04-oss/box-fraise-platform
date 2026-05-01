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
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

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

    for i in 1..=3u8 {
        let resp = app
            .clone()
            .oneshot(dorotka_request(&format!("fill {i}")))
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
            "request {i} must pass while window has capacity");
    }

    let resp = app.clone().oneshot(dorotka_request("overflow")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "4th request must be rate-limited (window is at capacity)");

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let resp = app.oneshot(dorotka_request("after window")).await.unwrap();
    assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "request after window elapses must not be rate-limited (sliding window reset)");
}
