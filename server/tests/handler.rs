//! Handler-level integration tests covering the full HTTP stack:
//! auth extractors, request parsing, response shape.
//! Each test uses `sqlx::test` for an isolated database.
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

fn json_req(method: &str, uri: &str, body: serde_json::Value) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(bytes))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

fn authed_req(method: &str, uri: &str, token: &str) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

fn authed_json_req(
    method: &str,
    uri: &str,
    token: &str,
    body: serde_json::Value,
) -> Request<Body> {
    let bytes = serde_json::to_vec(&body).unwrap();
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from(bytes))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

fn dorotka_request(query: &str) -> Request<Body> {
    json_req("POST", "/api/dorotka/ask", serde_json::json!({ "query": query }))
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth — GET /api/auth/me
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn auth_me_no_token_returns_401(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder().method("GET").uri("/api/auth/me")
        .body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn auth_me_expired_token_returns_401(pool: PgPool) {
    let user  = common::create_user(&pool, "expired@test.com").await;
    let token = common::expired_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/auth/me", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn auth_me_banned_user_returns_403(pool: PgPool) {
    let user = common::create_user(&pool, "banned@test.com").await;
    sqlx::query("UPDATE users SET is_banned = true WHERE id = $1")
        .bind(i32::from(user.id)).execute(&pool).await.unwrap();
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/auth/me", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn auth_me_valid_token_returns_200(pool: PgPool) {
    let user  = common::create_user(&pool, "me@test.com").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/auth/me", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth — POST /api/auth/apple
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn auth_apple_invalid_token_returns_401(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(json_req(
            "POST",
            "/api/auth/apple",
            serde_json::json!({ "identity_token": "not.a.real.jwt" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth — POST /api/auth/magic-link and verify
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn magic_link_request_valid_email_returns_200(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    // No Redis configured → returns Ok (silently skips token creation).
    let resp = app
        .oneshot(json_req(
            "POST",
            "/api/auth/magic-link",
            serde_json::json!({ "email": "test@example.com" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn magic_link_verify_missing_body_returns_400(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let mut req = Request::builder()
        .method("POST")
        .uri("/api/auth/magic-link/verify")
        .header("content-type", "application/json")
        .body(Body::from(b"{}".to_vec()))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    // Missing token field → validation fails.
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.status() == StatusCode::BAD_REQUEST
            || resp.status() == StatusCode::UNPROCESSABLE_ENTITY,
        "missing token must fail parsing, got: {}",
        resp.status()
    );
}

#[sqlx::test]
async fn magic_link_verify_invalid_token_returns_401(pool: PgPool) {
    // No Redis configured → Unauthorized (redis required but absent).
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(json_req(
            "POST",
            "/api/auth/magic-link/verify",
            serde_json::json!({ "token": "00000000-0000-0000-0000-000000000000" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth — PATCH /api/auth/display-name
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn display_name_no_auth_returns_401(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("PATCH")
        .uri("/api/auth/display-name")
        .header("content-type", "application/json")
        .body(Body::from(br#"{"display_name":"Alice"}"#.to_vec()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn display_name_authenticated_too_long_returns_400(pool: PgPool) {
    let user  = common::create_user(&pool, "dn@test.com").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let long_name = "a".repeat(51);
    let resp = app
        .oneshot(authed_json_req(
            "PATCH",
            "/api/auth/display-name",
            &token,
            serde_json::json!({ "display_name": long_name }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test]
async fn display_name_authenticated_valid_returns_200(pool: PgPool) {
    let user  = common::create_user(&pool, "dn2@test.com").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "PATCH",
            "/api/auth/display-name",
            &token,
            serde_json::json!({ "display_name": "Alice" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Users
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn users_search_no_auth_returns_401(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/search?q=alice")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn users_public_profile_unknown_returns_404(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/users/99999/public-profile")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─────────────────────────────────────────────────────────────────────────────
// Dorotka rate limit
// ─────────────────────────────────────────────────────────────────────────────

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
            "request {i} must not be rate-limited"
        );
    }

    let resp = app.oneshot(dorotka_request("query 21")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn dorotka_sliding_window_resets_after_window_elapses(pool: PgPool) {
    let state = common::build_state_with_dorotka_rate(pool.clone(), None, 3, 1);
    let app   = box_fraise_server::app::build(state);

    for i in 1..=3u8 {
        let resp = app.clone().oneshot(dorotka_request(&format!("fill {i}"))).await.unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
            "request {i} must pass while window has capacity");
    }

    let resp = app.clone().oneshot(dorotka_request("overflow")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let resp = app.oneshot(dorotka_request("after window")).await.unwrap();
    assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "request after window elapses must not be rate-limited");
}

// ─────────────────────────────────────────────────────────────────────────────
// Businesses
// ─────────────────────────────────────────────────────────────────────────────

fn business_body() -> serde_json::Value {
    serde_json::json!({
        "name":    "Test Café",
        "address": "123 Main St, Edmonton, AB"
    })
}

#[sqlx::test]
async fn post_businesses_returns_403_for_unattested_user(pool: PgPool) {
    let user  = common::create_user(&pool, "unattested@handler.test").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req("POST", "/api/businesses", &token, business_body()))
        .await
        .unwrap();
    // registered user (default) is not attested — must be Forbidden
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn post_businesses_returns_201_for_attested_user(pool: PgPool) {
    let user  = common::create_attested_user(&pool, "attested@handler.test").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req("POST", "/api/businesses", &token, business_body()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn get_businesses_me_returns_200(pool: PgPool) {
    let user  = common::create_attested_user(&pool, "me@handler.test").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/businesses/me", &token))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
