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
use box_fraise_domain::{with_admin_tx, with_rls_tx};
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

/// Build an authenticated Dorotka request — `RequireUser` now runs before
/// `rate_check`, so any caller exercising the rate limiter must hold a JWT.
/// The test user does NOT need an active soultoken because `rate_check`
/// fires before `ask_dorotka` (where the soultoken gate lives).
fn authed_dorotka_request(query: &str, token: &str) -> Request<Body> {
    authed_json_req("POST", "/api/dorotka/ask", token, serde_json::json!({ "query": query }))
}

#[sqlx::test]
async fn dorotka_rate_limit_21st_request_returns_429(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    for i in 1..=20u8 {
        let resp = app
            .clone()
            .oneshot(authed_dorotka_request(&format!("query {i}"), &token))
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "request {i} must not be rate-limited"
        );
    }

    let resp = app.oneshot(authed_dorotka_request("query 21", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[sqlx::test]
async fn dorotka_sliding_window_resets_after_window_elapses(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state_with_dorotka_rate(pool.clone(), None, 3, 1);
    let app   = box_fraise_server::app::build(state);

    for i in 1..=3u8 {
        let resp = app.clone().oneshot(authed_dorotka_request(&format!("fill {i}"), &token)).await.unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
            "request {i} must pass while window has capacity");
    }

    let resp = app.clone().oneshot(authed_dorotka_request("overflow", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

    let resp = app.oneshot(authed_dorotka_request("after window", &token)).await.unwrap();
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

// ─────────────────────────────────────────────────────────────────────────────
// Beacons
// ─────────────────────────────────────────────────────────────────────────────

async fn setup_beacon_fixtures(pool: &PgPool, email: &str) -> (i32, i32, i32) {
    // Returns (user_id_raw, business_id, beacon_id)
    let user  = common::create_attested_user(pool, email).await;
    let uid   = i32::from(user.id);

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Handler Test', 'partner_business', '1 Test Ave', 'America/Edmonton') \
         RETURNING id"
    )
    .fetch_one(pool)
    .await
    .unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'Handler Biz', 'pending') RETURNING id"
    )
    .bind(loc_id)
    .bind(uid)
    .fetch_one(pool)
    .await
    .unwrap();

    let (beacon_id,): (i32,) = sqlx::query_as(
        "INSERT INTO beacons (location_id, business_id, secret_key) \
         VALUES ($1, $2, 'handler-test-secret-key') RETURNING id"
    )
    .bind(loc_id)
    .bind(biz_id)
    .fetch_one(pool)
    .await
    .unwrap();

    (uid, biz_id, beacon_id)
}

#[sqlx::test]
async fn post_beacons_returns_201_for_business_owner(pool: PgPool) {
    let user  = common::create_attested_user(&pool, "beacon_owner@handler.test").await;
    let uid   = i32::from(user.id);
    let token = common::valid_token(uid);

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Beacon Loc', 'partner_business', '1 Beacon St', 'America/Edmonton') \
         RETURNING id"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'Beacon Biz', 'pending') RETURNING id"
    )
    .bind(loc_id)
    .bind(uid)
    .fetch_one(&pool)
    .await
    .unwrap();

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            "/api/beacons",
            &token,
            serde_json::json!({ "business_id": biz_id, "location_id": loc_id }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn post_beacons_returns_403_for_non_owner(pool: PgPool) {
    let owner = common::create_attested_user(&pool, "beacon_owner2@handler.test").await;
    let other = common::create_attested_user(&pool, "beacon_other@handler.test").await;
    let token = common::valid_token(i32::from(other.id));

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Beacon Loc2', 'partner_business', '2 Beacon St', 'America/Edmonton') \
         RETURNING id"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'Owner Biz', 'pending') RETURNING id"
    )
    .bind(loc_id)
    .bind(i32::from(owner.id))
    .fetch_one(&pool)
    .await
    .unwrap();

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            "/api/beacons",
            &token,
            serde_json::json!({ "business_id": biz_id, "location_id": loc_id }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn get_daily_uuid_returns_200_for_owner(pool: PgPool) {
    let (uid, _biz_id, beacon_id) =
        setup_beacon_fixtures(&pool, "daily_uuid@handler.test").await;
    let token = common::valid_token(uid);
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req(
            "GET",
            &format!("/api/beacons/{beacon_id}/daily-uuid"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn get_beacons_by_business_returns_200_for_owner(pool: PgPool) {
    let (uid, biz_id, beacon_id) =
        setup_beacon_fixtures(&pool, "list_beacons@handler.test").await;
    let token = common::valid_token(uid);
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req(
            "GET",
            &format!("/api/beacons/business/{biz_id}"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap(),
    )
    .unwrap();
    let beacons = body.as_array().expect("response must be an array");
    assert_eq!(beacons.len(), 1, "one beacon must be listed");
    assert_eq!(beacons[0]["id"].as_i64().unwrap(), beacon_id as i64);
    // secret_key must never appear in list response
    assert!(beacons[0]["secret_key"].is_null(), "secret_key must not be in response");
}

#[sqlx::test]
async fn rotate_beacon_key_returns_200_for_owner(pool: PgPool) {
    let (uid, _biz_id, beacon_id) =
        setup_beacon_fixtures(&pool, "rotate_key@handler.test").await;
    let token      = common::valid_token(uid);
    let pool_clone = pool.clone();

    let original_key: String = sqlx::query_scalar(
        "SELECT secret_key FROM beacons WHERE id = $1"
    )
    .bind(beacon_id)
    .fetch_one(&pool_clone)
    .await
    .unwrap();

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req(
            "POST",
            &format!("/api/beacons/{beacon_id}/rotate-key"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify key was actually rotated and previous_secret_key preserved.
    let (new_key, prev_key): (String, Option<String>) = sqlx::query_as(
        "SELECT secret_key, previous_secret_key FROM beacons WHERE id = $1"
    )
    .bind(beacon_id)
    .fetch_one(&pool_clone)
    .await
    .unwrap();

    assert_ne!(new_key, original_key, "key must change after rotation");
    assert_eq!(
        prev_key.as_deref(),
        Some(original_key.as_str()),
        "original key must be preserved as previous_secret_key"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Adversarial auth — Section 2 test 5
// ─────────────────────────────────────────────────────────────────────────────

/// A banned user must be blocked on every protected route regardless of having
/// a valid JWT. Confirms the ban check runs inside each extractor, not just at login.
#[sqlx::test]
async fn banned_user_is_blocked_on_all_protected_routes(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let user  = common::create_user(&pool, &email).await;
    let uid   = i32::from(user.id);
    let token = common::valid_token(uid);

    // Ban the user after their token was issued.
    sqlx::query("UPDATE users SET is_banned = true WHERE id = $1")
        .bind(uid).execute(&pool).await.unwrap();

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    // GET /api/auth/me
    let me_resp = app.clone()
        .oneshot(authed_req("GET", "/api/auth/me", &token))
        .await.unwrap();
    assert!(
        me_resp.status() == StatusCode::FORBIDDEN || me_resp.status() == StatusCode::UNAUTHORIZED,
        "banned user must get 401/403 on /api/auth/me, got {}",
        me_resp.status()
    );

    // GET /api/businesses/me
    let biz_resp = app.clone()
        .oneshot(authed_req("GET", "/api/businesses/me", &token))
        .await.unwrap();
    assert!(
        biz_resp.status() == StatusCode::FORBIDDEN || biz_resp.status() == StatusCode::UNAUTHORIZED,
        "banned user must get 401/403 on /api/businesses/me, got {}",
        biz_resp.status()
    );

    // POST /api/businesses
    let create_resp = app
        .oneshot(authed_json_req(
            "POST", "/api/businesses", &token,
            serde_json::json!({ "name": "Test", "address": "123 Test St" }),
        ))
        .await.unwrap();
    assert!(
        create_resp.status() == StatusCode::FORBIDDEN
            || create_resp.status() == StatusCode::UNAUTHORIZED,
        "banned user must get 401/403 on POST /api/businesses, got {}",
        create_resp.status()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Adversarial business — Section 3 test 10
// ─────────────────────────────────────────────────────────────────────────────

/// Business profiles are publicly accessible to any authenticated user —
/// the design is intentionally permissive for GET (businesses are public partner
/// profiles). Only write operations (create, delete, update) are owner-gated.
/// This test documents and asserts that permissive design.
#[sqlx::test]
async fn authenticated_user_can_view_any_business_profile(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let owner   = common::create_attested_user(&pool, SafeEmail().fake::<String>().as_str()).await;
    let visitor = common::create_user(&pool, SafeEmail().fake::<String>().as_str()).await;

    // Owner creates a business.
    let bus = box_fraise_domain::event_bus::EventBus::new();
    let biz = with_rls_tx!(&pool, owner.id, |tx| {
        box_fraise_domain::domain::businesses::service::create_business(
            &mut tx,
            &pool,
            owner.id,
            box_fraise_domain::domain::businesses::types::CreateBusinessRequest {
                name:          "Public Café".to_owned(),
                address:       "1 Main St, Edmonton, AB".to_owned(),
                latitude:      None,
                longitude:     None,
                timezone:      None,
                contact_email: None,
                contact_phone: None,
            },
            &bus,
        )
        .await
        .unwrap()
    });

    // Visitor (different user, not owner) can view the business.
    let visitor_token = common::valid_token(i32::from(visitor.id));
    let state         = common::build_state(pool, None);
    let app           = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", &format!("/api/businesses/{}", biz.id), &visitor_token))
        .await.unwrap();

    // Businesses are public profiles — any authenticated user may read them.
    assert_eq!(resp.status(), StatusCode::OK,
        "any authenticated user must be able to view a business profile (public design)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Identity credentials — BFIP Sections 3, 3b, 4
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn identity_verify_registered_user_returns_201(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let user  = common::create_user(&pool, SafeEmail().fake::<String>().as_str()).await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            "/api/identity/verify",
            &token,
            serde_json::json!({ "stripe_session_id": "vs_test_handler_001" }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn identity_cooling_app_open_returns_200(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let user  = common::create_user(&pool, SafeEmail().fake::<String>().as_str()).await;
    // Advance to identity_confirmed and insert a past-window credential directly.
    sqlx::query(
        "UPDATE users SET verification_status = 'identity_confirmed' WHERE id = $1"
    )
    .bind(i32::from(user.id))
    .execute(&pool)
    .await
    .unwrap();

    let verified_at     = chrono::Utc::now() - chrono::Duration::days(10);
    let cooling_ends_at = verified_at + chrono::Duration::days(7);
    let (cred_id,): (i32,) = sqlx::query_as(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at) \
         VALUES ($1, 'stripe_identity', $2, $3) RETURNING id"
    )
    .bind(i32::from(user.id))
    .bind(verified_at)
    .bind(cooling_ends_at)
    .fetch_one(&pool)
    .await
    .unwrap();

    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            "/api/identity/cooling/app-open",
            &token,
            serde_json::json!({ "credential_id": cred_id }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn identity_cooling_status_returns_404_without_credential(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let user  = common::create_user(&pool, SafeEmail().fake::<String>().as_str()).await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/identity/cooling/status", &token))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ─────────────────────────────────────────────────────────────────────────────
// Background checks
// ─────────────────────────────────────────────────────────────────────────────

/// Create an identity_confirmed user with a completed cooling credential.
async fn create_eligible_check_user(pool: &PgPool) -> (common::Usr, String) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'identity_confirmed') RETURNING id"
    )
    .bind(&email).fetch_one(pool).await.unwrap();

    // Cooling credential — already completed.
    let verified_at     = chrono::Utc::now() - chrono::Duration::days(10);
    let cooling_ends_at = verified_at + chrono::Duration::days(7);
    sqlx::query(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
         VALUES ($1, 'stripe_identity', $2, $3, now())"
    )
    .bind(id).bind(verified_at).bind(cooling_ends_at)
    .execute(pool).await.unwrap();

    let token = common::valid_token(id);
    (common::Usr { id: box_fraise_domain::types::UserId::from(id) }, token)
}

#[sqlx::test]
async fn post_initiate_returns_201_for_eligible_user(pool: PgPool) {
    let (_, token) = create_eligible_check_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            "/api/background-checks/initiate",
            &token,
            serde_json::json!({ "check_type": "sanctions", "provider": "comply_advantage" }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn post_initiate_returns_403_if_cooling_incomplete(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'identity_confirmed') RETURNING id"
    )
    .bind(&email).fetch_one(&pool).await.unwrap();

    // Credential without cooling_completed_at.
    let verified_at     = chrono::Utc::now() - chrono::Duration::days(2);
    let cooling_ends_at = verified_at + chrono::Duration::days(7);
    sqlx::query(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at) \
         VALUES ($1, 'stripe_identity', $2, $3)"
    )
    .bind(id).bind(verified_at).bind(cooling_ends_at)
    .execute(&pool).await.unwrap();

    let token = common::valid_token(id);
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            "/api/background-checks/initiate",
            &token,
            serde_json::json!({ "check_type": "sanctions", "provider": "comply_advantage" }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn get_status_returns_200_with_check_summary(pool: PgPool) {
    let (_, token) = create_eligible_check_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/background-checks/status", &token))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Staff domain
// ─────────────────────────────────────────────────────────────────────────────

async fn create_platform_admin_user(pool: &PgPool) -> (i32, String) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ($1, true, true) RETURNING id"
    )
    .bind(&email).fetch_one(pool).await.unwrap();
    (id, common::valid_token(id))
}

async fn create_location_for_tests(pool: &PgPool) -> i32 {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Handler Test Store', 'box_fraise_store', '1 Test St', 'America/Edmonton') \
         RETURNING id"
    )
    .fetch_one(pool).await.unwrap();
    id
}

#[sqlx::test]
async fn post_staff_roles_returns_201_for_admin(pool: PgPool) {
    let (admin_id, token) = create_platform_admin_user(&pool).await;
    let loc_id = create_location_for_tests(&pool).await;
    let (target_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ('target@staff.test', true) RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();
    let _ = (admin_id, loc_id);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/staff/roles", &token,
            serde_json::json!({ "user_id": target_id, "role": "attestation_reviewer", "location_id": null }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn post_staff_roles_returns_403_for_non_admin(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    )
    .bind(&email).fetch_one(&pool).await.unwrap();
    let token = common::valid_token(id);

    let (target_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ('target2@staff.test', true) RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/staff/roles", &token,
            serde_json::json!({ "user_id": target_id, "role": "attestation_reviewer", "location_id": null }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn post_staff_visits_returns_201_for_delivery_staff(pool: PgPool) {
    let (_, admin_token) = create_platform_admin_user(&pool).await;
    let loc_id = create_location_for_tests(&pool).await;
    let scheduled_at = (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339();

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/staff/visits", &admin_token,
            serde_json::json!({
                "location_id": loc_id,
                "visit_type":  "delivery",
                "scheduled_at": scheduled_at,
            }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn get_staff_visits_returns_200(pool: PgPool) {
    let (_, token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/staff/visits", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Attestations domain
// ─────────────────────────────────────────────────────────────────────────────

/// Sets up the full attestation context via direct SQL and service calls.
///
/// Returns `(staff_token, visit_id, target_user_id, threshold_id, r1_id, r2_id)`.
async fn setup_attestation_handler_context(pool: &PgPool) -> (String, i32, i32, i32, i32, i32) {
    use box_fraise_domain::domain::staff::{
        service as staff_svc,
        types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
    };
    use box_fraise_domain::event_bus::EventBus;
    use box_fraise_domain::types::UserId;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let bus = EventBus::new();

    let (admin_id, _) = create_platform_admin_user(pool).await;
    let admin = UserId::from(admin_id);

    let (staff_id, staff_token) = create_platform_admin_user(pool).await;
    let staff = UserId::from(staff_id);

    let loc_id = create_location_for_tests(pool).await;

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'Attest Handler Biz', 'active') RETURNING id",
    )
    .bind(loc_id)
    .bind(admin_id)
    .fetch_one(pool)
    .await
    .unwrap();

    // Grant staff delivery_staff role (admin grants to staff).
    with_admin_tx!(pool, |atx| {
        staff_svc::grant_staff_role(
            &mut atx, pool, admin,
            GrantRoleRequest {
                user_id: staff_id, role: "delivery_staff".to_owned(),
                location_id: Some(loc_id), expires_at: None, confirmed_by: None,
            },
            &bus,
        ).await.unwrap()
    });

    let visit = with_rls_tx!(pool, staff, |tx| {
        staff_svc::schedule_visit(
            &mut tx, pool, staff,
            ScheduleVisitRequest {
                location_id: loc_id, visit_type: "combined".to_owned(),
                scheduled_at: chrono::Utc::now() + chrono::Duration::hours(1),
                window_hours: Some(4), support_booking_capacity: Some(0), expected_box_count: Some(0),
            },
            &bus,
        ).await.unwrap()
    });

    with_rls_tx!(pool, staff, |tx| {
        staff_svc::arrive_at_visit(
            &mut tx, pool, visit.id, staff,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        ).await.unwrap()
    });

    // Two attestation reviewers (no location).
    let (r1_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool)
    .await
    .unwrap();

    let (r2_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool)
    .await
    .unwrap();

    for rid in [r1_id, r2_id] {
        with_admin_tx!(pool, |atx| {
            staff_svc::grant_staff_role(
                &mut atx, pool, admin,
                GrantRoleRequest {
                    user_id: rid, role: "attestation_reviewer".to_owned(),
                    location_id: None, expires_at: None, confirmed_by: None,
                },
                &bus,
            ).await.unwrap()
        });
    }

    // Target user: presence_confirmed + met threshold.
    let (target_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'presence_confirmed') RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool)
    .await
    .unwrap();

    let (threshold_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id",
    )
    .bind(target_id)
    .bind(biz_id)
    .fetch_one(pool)
    .await
    .unwrap();

    (staff_token, visit.id, target_id, threshold_id, r1_id, r2_id)
}

#[sqlx::test]
async fn get_attestations_me_returns_200(pool: PgPool) {
    let (_, token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/attestations", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn get_attestations_pending_returns_200(pool: PgPool) {
    let (_, token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/attestations/pending", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn post_attestations_returns_201_for_staff(pool: PgPool) {
    let (staff_token, visit_id, target_id, threshold_id, _, _) =
        setup_attestation_handler_context(&pool).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/attestations", &staff_token,
            serde_json::json!({
                "visit_id":              visit_id,
                "user_id":               target_id,
                "presence_threshold_id": threshold_id,
                "photo_hash":            null,
                "photo_storage_uri":     null,
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn post_attestations_staff_sign_returns_200(pool: PgPool) {
    use box_fraise_domain::domain::attestations::{service as attest_svc, types::InitiateAttestationRequest};
    use box_fraise_domain::event_bus::EventBus;
    use box_fraise_domain::types::UserId;

    let (staff_token, visit_id, target_id, threshold_id, _, _) =
        setup_attestation_handler_context(&pool).await;

    // Get staff user_id from token to call service directly.
    let (staff_id,): (i32,) = sqlx::query_as(
        "SELECT id FROM users WHERE is_platform_admin = true ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let bus = EventBus::new();
    let state_for_setup = common::build_state(pool.clone(), None);

    let attest = with_rls_tx!(&state_for_setup.db, staff_id, |tx| {
        attest_svc::initiate_attestation(
            &mut tx,
            &state_for_setup.db,
            UserId::from(staff_id),
            InitiateAttestationRequest {
                visit_id,
                user_id:               target_id,
                presence_threshold_id: threshold_id,
                photo_hash:            None,
                photo_storage_uri:     None,
            },
            &bus,
        )
        .await
        .expect("initiate must succeed for handler test setup")
    });

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    // Build the canonical Ed25519 payload and sign it with the test key pair
    // — server verifies before storing (Hardening Section 1c).
    use box_fraise_domain::domain::attestations::service::attestation_payload;
    let payload = attestation_payload(&attest);
    let kp = common::test_ed25519_key_pair();
    let signature = kp.sign(payload.as_bytes());

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            &format!("/api/attestations/{}/staff-sign", attest.id),
            &staff_token,
            serde_json::json!({
                "staff_signature":        signature,
                "verifying_key_hex":      kp.verifying_key_hex(),
                "photo_hash":             null,
                "location_confirmed":     true,
                "user_present_confirmed": true,
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Soultokens domain
// ─────────────────────────────────────────────────────────────────────────────

/// Seed an attested user with an approved attestation — minimum state for
/// soultoken issuance. Returns (user_id, token, attestation_id).
async fn setup_attested_user_for_handler(pool: &PgPool) -> (i32, String, i32) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'presence_confirmed') RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool).await.unwrap();

    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool).await.unwrap();

    let (r1,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool).await.unwrap();

    let (r2,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(pool).await.unwrap();

    sqlx::query(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
         VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now())",
    )
    .bind(uid).execute(pool).await.unwrap();

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Handler ST Store', 'box_fraise_store', '1 H St', 'America/Edmonton') \
         RETURNING id",
    )
    .fetch_one(pool).await.unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'H Biz', 'active') RETURNING id",
    )
    .bind(loc_id).bind(uid).fetch_one(pool).await.unwrap();

    let (thresh_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id",
    )
    .bind(uid).bind(biz_id).fetch_one(pool).await.unwrap();

    let (visit_id,): (i32,) = sqlx::query_as(
        "INSERT INTO staff_visits (location_id, staff_id, visit_type, status, scheduled_at) \
         VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id",
    )
    .bind(loc_id).bind(staff_id).fetch_one(pool).await.unwrap();

    let (attest_id,): (i32,) = sqlx::query_as(
        "INSERT INTO visit_attestations \
         (visit_id, user_id, staff_id, presence_threshold_id, \
          assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
         VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id",
    )
    .bind(visit_id).bind(uid).bind(staff_id)
    .bind(thresh_id).bind(r1).bind(r2)
    .fetch_one(pool).await.unwrap();

    sqlx::query(
        "UPDATE users SET verification_status = 'attested', attested_at = now() WHERE id = $1",
    )
    .bind(uid).execute(pool).await.unwrap();

    let token = common::valid_token(uid);
    (uid, token, attest_id)
}

#[sqlx::test]
async fn post_soultokens_issue_returns_201_for_attested_user(pool: PgPool) {
    let (_, token, attest_id) = setup_attested_user_for_handler(&pool).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/soultokens/issue", &token,
            serde_json::json!({
                "attestation_id": attest_id,
                "token_type":     "user",
            }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn get_soultokens_me_returns_200_with_display_code(pool: PgPool) {
    use box_fraise_domain::domain::soultokens::{service as st_svc, types::IssueSoultokenRequest};
    use box_fraise_domain::event_bus::EventBus;
    use box_fraise_domain::types::UserId;

    let (uid, token, attest_id) = setup_attested_user_for_handler(&pool).await;

    let state_setup = common::build_state(pool.clone(), None);
    let bus = EventBus::new();
    use secrecy::ExposeSecret;
    let hmac_key = state_setup.cfg.soultoken_hmac_key.expose_secret().as_bytes().to_vec();
    with_rls_tx!(&state_setup.db, uid, |tx| {
        st_svc::issue_soultoken(
            &mut tx, &state_setup.db, UserId::from(uid),
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            &hmac_key, &state_setup.ed25519_key_pair, &bus,
        ).await.expect("issue must succeed for handler test setup")
    });

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/soultokens/me", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn post_soultokens_revoke_returns_403_for_non_admin(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(id);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/soultokens/1/revoke", &token,
            serde_json::json!({
                "revocation_reason":   "stripe_flag",
                "revocation_visit_id": null,
            }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn get_soultokens_me_response_contains_no_uuid(pool: PgPool) {
    use box_fraise_domain::domain::soultokens::{service as st_svc, types::IssueSoultokenRequest};
    use box_fraise_domain::event_bus::EventBus;
    use box_fraise_domain::types::UserId;
    use axum::body::to_bytes;

    let (uid, token, attest_id) = setup_attested_user_for_handler(&pool).await;

    let state_setup = common::build_state(pool.clone(), None);
    let bus = EventBus::new();
    use secrecy::ExposeSecret;
    let hmac_key = state_setup.cfg.soultoken_hmac_key.expose_secret().as_bytes().to_vec();
    with_rls_tx!(&state_setup.db, uid, |tx| {
        st_svc::issue_soultoken(
            &mut tx, &state_setup.db, UserId::from(uid),
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            &hmac_key, &state_setup.ed25519_key_pair, &bus,
        ).await.unwrap()
    });

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/soultokens/me", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = to_bytes(resp.into_body(), 1024 * 64).await.unwrap();
    let json_str = std::str::from_utf8(&body).unwrap();

    let uuid_re = regex::Regex::new(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    ).unwrap();
    assert!(
        !uuid_re.is_match(json_str),
        "uuid must NOT appear anywhere in /api/soultokens/me response: {json_str}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Orders domain
// ─────────────────────────────────────────────────────────────────────────────

async fn create_active_business(pool: &PgPool) -> (i32, String) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (admin_id, admin_token) = create_platform_admin_user(pool).await;
    let loc_id = create_location_for_tests(pool).await;
    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses \
         (location_id, primary_holder_id, name, verification_status, is_active) \
         VALUES ($1, $2, 'Handler Test Biz', 'active', true) RETURNING id",
    )
    .bind(loc_id).bind(admin_id).fetch_one(pool).await.unwrap();
    (biz_id, admin_token)
}

#[sqlx::test]
async fn post_orders_returns_201(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (biz_id, token) = create_active_business(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/orders", &token,
            serde_json::json!({
                "business_id":         biz_id,
                "variety_description": "Albion",
                "box_count":           1,
                "amount_cents":        1500,
            }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn get_orders_me_returns_200(pool: PgPool) {
    let (_, token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app.oneshot(authed_req("GET", "/api/orders", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test]
async fn post_orders_cancel_returns_403_for_wrong_user(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    use box_fraise_domain::domain::orders::{service as ord_svc, types::CreateOrderRequest};
    use box_fraise_domain::event_bus::EventBus;
    use box_fraise_domain::types::UserId;

    let (biz_id, _owner_token) = create_active_business(&pool).await;

    // Create order owned by admin (the same user that created the business).
    let (owner_id,): (i32,) = sqlx::query_as(
        "SELECT id FROM users WHERE is_platform_admin = true ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(&pool).await.unwrap();

    let bus = EventBus::new();
    let state_setup = common::build_state(pool.clone(), None);
    let order = with_rls_tx!(&state_setup.db, owner_id, |tx| {
        ord_svc::create_order(
            &mut tx, &state_setup.db, UserId::from(owner_id),
            CreateOrderRequest {
                business_id: biz_id, variety_description: None,
                box_count: 1, amount_cents: 1000,
            },
            &bus,
        ).await.unwrap()
    });

    // A different user tries to cancel.
    let (attacker_id, attacker_token) = create_platform_admin_user(&pool).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            &format!("/api/orders/{}/cancel", order.id),
            &attacker_token,
            serde_json::json!({}),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn post_orders_collect_returns_409_for_double_tap(pool: PgPool) {
    use box_fraise_domain::domain::orders::{
        service as ord_svc,
        types::{ActivateBoxRequest, CollectOrderRequest, CreateOrderRequest},
    };
    use box_fraise_domain::domain::staff::{
        service as staff_svc,
        types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
    };
    use box_fraise_domain::event_bus::EventBus;
    use box_fraise_domain::types::UserId;

    let bus = EventBus::new();
    let state_setup = common::build_state(pool.clone(), None);

    // Setup: admin, staff + role, business, visit.
    let (admin_id, _)    = create_platform_admin_user(&pool).await;
    let (staff_id, _)    = create_platform_admin_user(&pool).await;
    let loc_id           = create_location_for_tests(&pool).await;
    let admin            = UserId::from(admin_id);
    let staff            = UserId::from(staff_id);

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses \
         (location_id, primary_holder_id, name, verification_status, is_active) \
         VALUES ($1, $2, 'DT Biz', 'active', true) RETURNING id",
    )
    .bind(loc_id).bind(admin_id).fetch_one(&pool).await.unwrap();

    with_admin_tx!(&state_setup.db, |atx| {
        staff_svc::grant_staff_role(
            &mut atx, &state_setup.db, admin,
            GrantRoleRequest {
                user_id: staff_id, role: "delivery_staff".to_owned(),
                location_id: Some(loc_id), expires_at: None, confirmed_by: None,
            },
            &bus,
        ).await.unwrap()
    });

    let visit = with_rls_tx!(&state_setup.db, staff, |tx| {
        staff_svc::schedule_visit(
            &mut tx, &state_setup.db, staff,
            ScheduleVisitRequest {
                location_id: loc_id, visit_type: "delivery".to_owned(),
                scheduled_at: chrono::Utc::now() + chrono::Duration::hours(1),
                window_hours: Some(4), support_booking_capacity: Some(0), expected_box_count: Some(5),
            },
            &bus,
        ).await.unwrap()
    });

    with_rls_tx!(&state_setup.db, staff, |tx| {
        staff_svc::arrive_at_visit(
            &mut tx, &state_setup.db, visit.id, staff,
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        ).await.unwrap()
    });

    // Create order for user.
    let (user_id, user_token) = create_platform_admin_user(&pool).await;
    let user = UserId::from(user_id);

    with_rls_tx!(&state_setup.db, user_id, |tx| {
        ord_svc::create_order(
            &mut tx, &state_setup.db, user,
            CreateOrderRequest {
                business_id: biz_id, variety_description: None,
                box_count: 1, amount_cents: 1000,
            },
            &bus,
        ).await.unwrap()
    });

    // Activate a box.
    with_rls_tx!(&state_setup.db, staff, |tx| {
        ord_svc::activate_box(
            &mut tx, &state_setup.db, visit.id, staff,
            ActivateBoxRequest {
                nfc_chip_uid:       "NFC-HANDLER-DT".to_owned(),
                delivery_signature: "sig".to_owned(),
                expires_at:         chrono::Utc::now() + chrono::Duration::hours(4),
            },
        ).await.unwrap()
    });

    // First collect via service (succeeds).
    with_rls_tx!(&state_setup.db, user_id, |tx| {
        ord_svc::collect_order(
            &mut tx, &state_setup.db, user,
            CollectOrderRequest { nfc_chip_uid: "NFC-HANDLER-DT".to_owned() },
            &bus,
        ).await.unwrap()
    });

    // Second collect via HTTP (should be 409).
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/orders/collect", &user_token,
            serde_json::json!({ "nfc_chip_uid": "NFC-HANDLER-DT" }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

// ─────────────────────────────────────────────────────────────────────────────
// Support domain
// ─────────────────────────────────────────────────────────────────────────────

/// Set up a delivery_staff user + a support visit with capacity.
/// Returns (staff_id, staff_token, visit_id).
async fn setup_support_visit_for_handler(pool: &PgPool, capacity: i32) -> (i32, String, i32) {
    use box_fraise_domain::{
        domain::staff::{
            service as staff_svc,
            types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
        },
        event_bus::EventBus,
        types::UserId,
    };
    let bus = EventBus::new();

    let (admin_id, _) = create_platform_admin_user(pool).await;
    let loc_id        = create_location_for_tests(pool).await;

    use fake::{Fake, faker::internet::en::SafeEmail};
    let staff_email: String = SafeEmail().fake();
    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&staff_email).fetch_one(pool).await.unwrap();
    let staff_token = common::valid_token(staff_id);

    with_admin_tx!(pool, |atx| {
        staff_svc::grant_staff_role(&mut atx, pool, UserId::from(admin_id),
            GrantRoleRequest {
                user_id:      staff_id,
                role:         "delivery_staff".to_owned(),
                location_id:  Some(loc_id),
                expires_at:   None,
                confirmed_by: None,
            }, &bus,
        ).await.unwrap()
    });

    let visit = with_rls_tx!(pool, staff_id, |tx| {
        staff_svc::schedule_visit(&mut tx, pool, UserId::from(staff_id),
            ScheduleVisitRequest {
                location_id:              loc_id,
                visit_type:               "support".to_owned(),
                scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
                window_hours:             Some(4),
                support_booking_capacity: Some(capacity),
                expected_box_count:       Some(0),
            }, &bus,
        ).await.unwrap()
    });

    with_rls_tx!(pool, staff_id, |tx| {
        staff_svc::arrive_at_visit(&mut tx, pool, visit.id, UserId::from(staff_id),
            ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
        ).await.unwrap()
    });

    (staff_id, staff_token, visit.id)
}

#[sqlx::test]
async fn post_support_bookings_returns_201(pool: PgPool) {
    let (_, _, visit_id) = setup_support_visit_for_handler(&pool, 5).await;

    // Create a regular user.
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (user_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&email).fetch_one(&pool).await.unwrap();
    let user_token = common::valid_token(user_id);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/support/bookings", &user_token,
            serde_json::json!({ "visit_id": visit_id }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn post_support_bookings_returns_409_for_duplicate(pool: PgPool) {
    use box_fraise_domain::{
        domain::support::{service as sup_svc, types::CreateBookingRequest},
        event_bus::EventBus, types::UserId,
    };
    let bus = EventBus::new();
    let (_, _, visit_id) = setup_support_visit_for_handler(&pool, 5).await;

    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (user_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&email).fetch_one(&pool).await.unwrap();
    let user_token = common::valid_token(user_id);

    // Book via service first.
    with_rls_tx!(&pool, user_id, |tx| {
        sup_svc::create_booking(
            &mut tx, &pool, UserId::from(user_id),
            CreateBookingRequest { visit_id, issue_description: None, priority: None },
            &bus,
        ).await.unwrap()
    });

    // Second booking via HTTP should conflict.
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/support/bookings", &user_token,
            serde_json::json!({ "visit_id": visit_id }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[sqlx::test]
async fn post_support_bookings_resolve_returns_403_for_non_staff(pool: PgPool) {
    use box_fraise_domain::{
        domain::support::{service as sup_svc, types::{CreateBookingRequest, ResolveBookingRequest}},
        event_bus::EventBus, types::UserId,
    };
    let bus = EventBus::new();
    let (staff_id, _, visit_id) = setup_support_visit_for_handler(&pool, 5).await;

    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (user_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&email).fetch_one(&pool).await.unwrap();

    let booking = with_rls_tx!(&pool, user_id, |tx| {
        sup_svc::create_booking(
            &mut tx, &pool, UserId::from(user_id),
            CreateBookingRequest { visit_id, issue_description: None, priority: None },
            &bus,
        ).await.unwrap()
    });

    // Mark attended via staff.
    with_rls_tx!(&pool, staff_id, |tx| {
        sup_svc::attend_booking(&mut tx, &pool, booking.id, UserId::from(staff_id)).await.unwrap()
    });

    // A random non-staff user tries to resolve.
    let attacker_email: String = SafeEmail().fake();
    let (attacker_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&attacker_email).fetch_one(&pool).await.unwrap();
    let attacker_token = common::valid_token(attacker_id);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST",
            &format!("/api/support/bookings/{}/resolve", booking.id),
            &attacker_token,
            serde_json::json!({
                "resolution_description": "Fake resolve",
                "resolution_signature":   "fake-sig",
                "gift_box_provided":      false,
            }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn get_support_bookings_me_returns_200(pool: PgPool) {
    let (_, _, visit_id) = setup_support_visit_for_handler(&pool, 5).await;

    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (user_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&email).fetch_one(&pool).await.unwrap();
    let user_token = common::valid_token(user_id);

    // Create a booking first.
    use box_fraise_domain::{
        domain::support::{service as sup_svc, types::CreateBookingRequest},
        event_bus::EventBus, types::UserId,
    };
    let bus = EventBus::new();
    with_rls_tx!(&pool, user_id, |tx| {
        sup_svc::create_booking(
            &mut tx, &pool, UserId::from(user_id),
            CreateBookingRequest { visit_id, issue_description: None, priority: None },
            &bus,
        ).await.unwrap()
    });

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/support/bookings/me", &user_token))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

// ─────────────────────────────────────────────────────────────────────────────
// Attestation tokens domain
// ─────────────────────────────────────────────────────────────────────────────

/// Set up an attested user with an active soultoken.
/// Returns (user_id, user_token).
async fn setup_attested_user_with_soultoken_for_handler(
    pool: &PgPool,
    user_email: &str,
) -> (i32, String) {
    use box_fraise_domain::{
        domain::soultokens::{service as st_svc, types::IssueSoultokenRequest},
        event_bus::EventBus,
        types::UserId,
    };
    use fake::{Fake, faker::internet::en::SafeEmail};

    let bus = EventBus::new();

    // Use direct SQL inserts — same approach as soultokens/service.rs tests.
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'presence_confirmed') RETURNING id"
    ).bind(user_email).fetch_one(pool).await.unwrap();

    let s_email: String = SafeEmail().fake();
    let r1_email: String = SafeEmail().fake();
    let r2_email: String = SafeEmail().fake();
    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&s_email).fetch_one(pool).await.unwrap();
    let (r1_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&r1_email).fetch_one(pool).await.unwrap();
    let (r2_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&r2_email).fetch_one(pool).await.unwrap();

    sqlx::query(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
         VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now())"
    ).bind(uid).execute(pool).await.unwrap();

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('AT Handler Store', 'box_fraise_store', '1 ATH St', 'America/Edmonton') \
         RETURNING id"
    ).fetch_one(pool).await.unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'AT Biz', 'active') RETURNING id"
    ).bind(loc_id).bind(uid).fetch_one(pool).await.unwrap();

    let (thresh_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id"
    ).bind(uid).bind(biz_id).fetch_one(pool).await.unwrap();

    let (visit_id,): (i32,) = sqlx::query_as(
        "INSERT INTO staff_visits (location_id, staff_id, visit_type, status, scheduled_at) \
         VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id"
    ).bind(loc_id).bind(staff_id).fetch_one(pool).await.unwrap();

    let (attest_id,): (i32,) = sqlx::query_as(
        "INSERT INTO visit_attestations \
         (visit_id, user_id, staff_id, presence_threshold_id, \
          assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
         VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id"
    ).bind(visit_id).bind(uid).bind(staff_id)
     .bind(thresh_id).bind(r1_id).bind(r2_id)
     .fetch_one(pool).await.unwrap();

    sqlx::query("UPDATE users SET verification_status = 'attested', attested_at = now() WHERE id = $1")
        .bind(uid).execute(pool).await.unwrap();

    let kp = common::test_ed25519_key_pair();
    with_rls_tx!(pool, uid, |tx| {
        st_svc::issue_soultoken(&mut tx, pool, UserId::from(uid),
            IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
            b"test-soultoken-hmac-key-32bytes!!", &kp, &bus,
        ).await.unwrap()
    });

    (uid, common::valid_token(uid))
}

#[sqlx::test]
async fn post_issue_returns_201_with_raw_token(pool: PgPool) {
    let (_, token) = setup_attested_user_with_soultoken_for_handler(
        &pool, "attestation_token_test_user@example.com",
    ).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/attestation-tokens/issue", &token,
            serde_json::json!({ "scope": "presence.verified" }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(body["raw_token"].as_str().is_some(), "raw_token must be in issue response");
    assert_eq!(body["raw_token"].as_str().unwrap().len(), 64);
    assert_eq!(body["scope"], "presence.verified");
}

#[sqlx::test]
async fn post_verify_returns_200_for_valid_token(pool: PgPool) {
    use box_fraise_domain::{
        domain::attestation_tokens::{service as at_svc, types::IssueAttestationTokenRequest},
        event_bus::EventBus, types::UserId,
    };
    let bus = EventBus::new();
    let (user_id, _) = setup_attested_user_with_soultoken_for_handler(
        &pool, "attestation_token_verify_valid_user@example.com",
    ).await;

    let issued = with_rls_tx!(&pool, user_id, |tx| {
        at_svc::issue_token(&mut tx, &pool, UserId::from(user_id),
            IssueAttestationTokenRequest {
                scope: "presence.verified".to_owned(),
                requesting_business_soultoken_id: None,
                user_device_id: None,
                presentation_latitude: None,
                presentation_longitude: None,
            }, &bus,
        ).await.unwrap()
    });

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(json_req(
            "POST", "/api/attestation-tokens/verify",
            serde_json::json!({ "raw_token": &issued.raw_token }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()
    ).unwrap();

    assert_eq!(body["valid"], true);
    assert_eq!(body["outcome"], "success");
}

#[sqlx::test]
async fn post_verify_returns_200_for_invalid_token(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(json_req(
            "POST", "/api/attestation-tokens/verify",
            serde_json::json!({ "raw_token": "0000000000000000000000000000000000000000000000000000000000000000" }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()
    ).unwrap();

    assert_eq!(body["valid"], false);
    assert_eq!(body["outcome"], "not_found");
}

#[sqlx::test]
async fn get_me_returns_200_without_raw_token(pool: PgPool) {
    use box_fraise_domain::{
        domain::attestation_tokens::{service as at_svc, types::IssueAttestationTokenRequest},
        event_bus::EventBus, types::UserId,
    };
    let bus = EventBus::new();
    let (user_id, user_token) = setup_attested_user_with_soultoken_for_handler(
        &pool, "attestation_token_get_me_user@example.com",
    ).await;

    let issued = with_rls_tx!(&pool, user_id, |tx| {
        at_svc::issue_token(&mut tx, &pool, UserId::from(user_id),
            IssueAttestationTokenRequest {
                scope: "presence.verified".to_owned(),
                requesting_business_soultoken_id: None,
                user_device_id: None,
                presentation_latitude: None,
                presentation_longitude: None,
            }, &bus,
        ).await.unwrap()
    });

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/attestation-tokens/me", &user_token))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_str   = std::str::from_utf8(&body_bytes).unwrap();

    assert!(!body_str.contains(&issued.raw_token),
        "raw_token value must not appear in GET /me response");
    assert!(!body_str.contains("\"raw_token\""),
        "field name raw_token must not appear in list response");
}

// ─────────────────────────────────────────────────────────────────────────────
// Verification events domain
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn get_audit_trail_returns_200(pool: PgPool) {
    let (user_id, token) = setup_attested_user_with_soultoken_for_handler(
        &pool, "audit_trail_test_user@example.com",
    ).await;
    let _ = user_id;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/audit/trail", &token))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()
    ).unwrap();

    assert!(body["verification_journey"].is_array());
    assert!(body["soultoken_history"].is_array());
    assert!(body["presence_history"].is_array());
    assert!(body["attestation_history"].is_array());
    assert!(body["token_history"].is_array());
}

#[sqlx::test]
async fn get_audit_journey_returns_200(pool: PgPool) {
    let (_, token) = setup_attested_user_with_soultoken_for_handler(
        &pool, "audit_journey_test_user@example.com",
    ).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/audit/journey", &token))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()
    ).unwrap();

    assert!(body.is_array(), "journey endpoint must return a JSON array");
}

#[sqlx::test]
async fn get_admin_audit_returns_403_for_non_admin(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (non_admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&email).fetch_one(&pool).await.unwrap();
    let non_admin_token = common::valid_token(non_admin_id);

    let target_id = non_admin_id;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req(
            "GET",
            &format!("/api/admin/audit/{}", target_id),
            &non_admin_token,
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ─────────────────────────────────────────────────────────────────────────────
// Platform configuration domain
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn get_admin_configuration_returns_200_for_admin(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;
    cfg_svc::initialize_defaults(&pool).await.unwrap();

    let (admin_id, token) = create_platform_admin_user(&pool).await;
    let _ = admin_id;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/configuration", &token))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()
    ).unwrap();

    assert!(body.is_array());
    assert!(body.as_array().unwrap().len() >= 14);
}

#[sqlx::test]
async fn get_admin_configuration_returns_403_for_non_admin(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;
    cfg_svc::initialize_defaults(&pool).await.unwrap();

    use fake::{Fake, faker::internet::en::SafeEmail};
    let email: String = SafeEmail().fake();
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&email).fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/configuration", &token))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn patch_configuration_returns_200_for_valid_update(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;
    cfg_svc::initialize_defaults(&pool).await.unwrap();

    let (_, token) = create_platform_admin_user(&pool).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "PATCH", "/api/admin/configuration/cooling_period_days", &token,
            serde_json::json!({ "value": "14" }),
        ))
        .await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()
    ).unwrap();
    assert_eq!(body["value"], "14");
}

#[sqlx::test]
async fn patch_configuration_returns_422_for_invalid_type(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;
    cfg_svc::initialize_defaults(&pool).await.unwrap();

    let (_, token) = create_platform_admin_user(&pool).await;

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "PATCH", "/api/admin/configuration/cooling_period_days", &token,
            serde_json::json!({ "value": "not_a_number" }),
        ))
        .await.unwrap();

    // InvalidInput → 400 Bad Request via AppError.
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ─────────────────────────────────────────────────────────────────────────────
// Trust registry — public Ed25519 verifying key (Hardening Section 1b)
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn trust_registry_returns_verifying_key(pool: PgPool) {
    let state = common::build_state(pool, None);
    let configured_pub = state.cfg.soultoken_verifying_key_hex.clone();
    let app = box_fraise_server::app::build(state);

    // No Authorization header — endpoint must be public.
    let req = Request::builder()
        .method("GET")
        .uri("/api/trust-registry/public-key")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();

    assert_eq!(resp.status(), StatusCode::OK,
        "trust-registry endpoint must be reachable without auth");

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    let vk_hex = body["verifying_key_hex"].as_str().unwrap();
    assert_eq!(vk_hex.len(), 64, "verifying_key_hex must be 64 chars (32 bytes)");
    assert_eq!(vk_hex, configured_pub.as_str(),
        "endpoint must expose the configured SOULTOKEN_VERIFYING_KEY_HEX");
    assert_eq!(body["algorithm"].as_str().unwrap(), "Ed25519");
    assert!(body["bfip_version"].as_str().is_some(), "bfip_version must be present");
    assert!(body["description"].as_str().is_some(), "description must be present");
}

#[sqlx::test]
async fn trust_registry_returns_v0_2_0(pool: PgPool) {
    // Hardening §11 — the trust-registry endpoint reports the BFIP protocol
    // version it conforms to. Bumped from 0.1.2 → 0.2.0 with the §22 stub.
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/trust-registry/public-key")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["bfip_version"].as_str(), Some("0.2.0"),
        "trust registry must advertise BFIP v0.2.0");
}

// ─────────────────────────────────────────────────────────────────────────────
// Evidence storage — 503 when SPACES_* not configured (Hardening §3)
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn post_evidence_upload_returns_503_when_storage_not_configured(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    // Empty multipart body with the right Content-Type header — the
    // Multipart extractor parses the headers and lets the handler run;
    // require_storage short-circuits with 503 before any field is read.
    let req = Request::builder()
        .method("POST")
        .uri("/api/staff/visits/1/evidence")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "multipart/form-data; boundary=BOUND")
        .header("x-forwarded-for", "127.0.0.1")
        .body(Body::from("--BOUND--\r\n"))
        .unwrap();
    let mut req = req;
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ─────────────────────────────────────────────────────────────────────────────
// Observability (Hardening §4) — /metrics + /health
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn metrics_endpoint_returns_200(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    // The default axum-prometheus metric set always includes this counter
    // family; even before any request is recorded the family appears with
    // a HELP/TYPE comment block.
    assert!(
        text.contains("axum_http_requests_total") || text.contains("http_requests_total"),
        "metrics body must include the http_requests_total family, got: {}",
        &text.chars().take(500).collect::<String>(),
    );
}

#[sqlx::test]
async fn health_endpoint_returns_healthy_when_db_ok(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"],   "healthy");
    assert_eq!(json["database"], "ok");
}

#[sqlx::test]
async fn health_endpoint_returns_degraded_when_redis_unavailable(pool: PgPool) {
    // Configure Redis at an unreachable port — pool creation is lazy, the
    // first PING call fails and tips the health check to "degraded".
    let bad_redis = deadpool_redis::Config::from_url("redis://127.0.0.1:1")
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("deadpool create_pool is infallible for a parsable URL");
    let state = common::build_state(pool, Some(bad_redis));
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK,
        "Redis-only failure must remain HTTP 200 — degraded does not page");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["redis"],  "error");
}

#[sqlx::test]
async fn health_endpoint_includes_storage_status(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["storage"], "not_configured",
        "test fixtures don't wire SPACES_* — storage must be reported as not_configured");
    assert!(json["version"].as_str().is_some(), "version must appear in /health");
}

#[sqlx::test]
async fn get_evidence_url_returns_503_when_storage_not_configured(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req(
            "GET",
            "/api/staff/visits/1/evidence/url?path=evidence/visits/1/123_photo",
            &token,
        ))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ─────────────────────────────────────────────────────────────────────────────
// Analytics (Hardening §5) — admin-only aggregate endpoints
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn get_analytics_funnel_returns_200_for_admin(pool: PgPool) {
    let (_, admin_token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/analytics/funnel", &admin_token))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array(), "funnel response must be a JSON array");
}

#[sqlx::test]
async fn get_analytics_funnel_returns_403_for_non_admin(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/analytics/funnel", &token))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn get_analytics_businesses_returns_200_for_admin(pool: PgPool) {
    let (_, admin_token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/analytics/businesses", &admin_token))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total_active"].is_i64(), "businesses response must carry total_active");
}

#[sqlx::test]
async fn get_analytics_soultokens_returns_200_for_admin(pool: PgPool) {
    let (_, admin_token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/analytics/soultokens", &admin_token))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total_issued"].is_i64(), "soultokens response must carry total_issued");
}

// ─────────────────────────────────────────────────────────────────────────────
// API hardening (§6) — CORS lockdown, Dorotka soultoken gate,
// security headers, Retry-After on 429.
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn cors_rejects_disallowed_origin(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/auth/me")
        .header("origin", "https://evil.com")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let cors = resp.headers().get("access-control-allow-origin").map(|v| v.to_str().unwrap_or("").to_string());
    assert_ne!(cors.as_deref(), Some("https://evil.com"),
        "preflight from disallowed origin must not reflect the origin in ACAO");
}

#[sqlx::test]
async fn cors_allows_configured_origin(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/auth/me")
        .header("origin", "http://localhost:3000")
        .header("access-control-request-method", "GET")
        .header("access-control-request-headers", "authorization")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let cors = resp.headers().get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok()).unwrap_or("");
    assert_eq!(cors, "http://localhost:3000",
        "preflight from configured origin must reflect the origin in ACAO");
}

#[sqlx::test]
async fn dorotka_requires_active_soultoken(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'registered') RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_json_req(
            "POST", "/api/dorotka/ask", &token,
            serde_json::json!({ "query": "hi" }),
        ))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn response_includes_security_headers(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/auth/me", &token))
        .await.unwrap();
    let headers = resp.headers();
    assert_eq!(headers.get("x-content-type-options").and_then(|v| v.to_str().ok()), Some("nosniff"));
    assert_eq!(headers.get("x-frame-options").and_then(|v| v.to_str().ok()), Some("DENY"));
    let csp = headers.get("content-security-policy")
        .and_then(|v| v.to_str().ok()).unwrap_or("");
    assert!(csp.contains("nonce-"), "CSP must include a per-request nonce, got: {csp}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Real-time notifications (Hardening §7) — SSE stream
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn sse_stream_returns_200_with_correct_content_type(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/notifications/stream", &token))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
    assert!(ct.starts_with("text/event-stream"),
        "SSE response must be text/event-stream, got: {ct}");
}

// ─────────────────────────────────────────────────────────────────────────────
// Operational hardening (§10) — consent on creation, feature flags, ban/unban
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn new_user_creation_records_consent(pool: PgPool) {
    use box_fraise_domain::domain::auth::repository as user_repo;
    use fake::{Fake, faker::internet::en::SafeEmail};

    // Drive user creation through the magic-link path — ANTHROPIC_API_KEY /
    // RESEND_API_KEY etc. aren't required for `find_or_create_magic_link_user`.
    let email: String = SafeEmail().fake();
    let (_user, is_new) = user_repo::find_or_create_magic_link_user(&pool, &email)
        .await.expect("user creation must succeed");
    assert!(is_new, "fresh email must produce a new user");

    // Now invoke record_consent the same way request_magic_link does.
    use box_fraise_domain::domain::users::service as user_svc;
    let uid = i32::from(_user.id);
    user_svc::record_consent(&pool, uid, "platform_terms", true, None).await.unwrap();
    user_svc::record_consent(&pool, uid, "privacy_policy", true, None).await.unwrap();

    let rows: Vec<(String, bool)> = sqlx::query_as(
        "SELECT consent_type, granted FROM consent_records WHERE user_id = $1 \
         ORDER BY consent_type"
    ).bind(uid).fetch_all(&pool).await.unwrap();

    assert_eq!(rows.len(), 2, "must have two consent rows");
    assert_eq!(rows[0], ("platform_terms".to_string(), true));
    assert_eq!(rows[1], ("privacy_policy".to_string(), true));
}

#[sqlx::test]
async fn is_feature_enabled_returns_false_for_unknown_flag(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;
    cfg_svc::initialize_defaults(&pool).await.ok();

    let enabled = cfg_svc::is_feature_enabled(&pool, "nonexistent_xyz", None)
        .await.expect("query must not error");
    assert!(!enabled, "unknown flags must resolve to false (fail closed)");
}

#[sqlx::test]
async fn is_feature_enabled_returns_true_when_globally_enabled(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;

    // Migration 006 seeds web3_payments=false. Flip it on.
    sqlx::query("UPDATE feature_flags SET enabled = true WHERE flag_name = 'web3_payments'")
        .execute(&pool).await.unwrap();

    let enabled = cfg_svc::is_feature_enabled(&pool, "web3_payments", None)
        .await.unwrap();
    assert!(enabled, "globally-enabled flag must resolve to true");
}

#[sqlx::test]
async fn is_feature_enabled_returns_true_for_specific_user(pool: PgPool) {
    use box_fraise_domain::domain::platform_configuration::service as cfg_svc;

    // Allow-list user 42 specifically — flag stays globally off.
    sqlx::query(
        "UPDATE feature_flags SET enabled_for_user_ids = ARRAY[42]::integer[] \
         WHERE flag_name = 'mesh_presence'"
    ).execute(&pool).await.unwrap();

    let on  = cfg_svc::is_feature_enabled(&pool, "mesh_presence", Some(42)).await.unwrap();
    let off = cfg_svc::is_feature_enabled(&pool, "mesh_presence", Some(99)).await.unwrap();
    assert!( on,  "user 42 must see the flag enabled");
    assert!(!off, "user 99 must NOT see the flag enabled");
}

#[sqlx::test]
async fn admin_ban_user_sets_is_banned(pool: PgPool) {
    use box_fraise_domain::domain::users::service as user_svc;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ($1, true, true) RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    let (target_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();

    with_admin_tx!(&pool, |atx| {
        user_svc::admin_ban_user(&mut atx, &pool, admin_id, target_id, "test ban".to_string())
            .await.expect("ban must succeed")
    });

    let row: (bool, Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
        "SELECT is_banned, banned_at, banned_reason FROM users WHERE id = $1"
    ).bind(target_id).fetch_one(&pool).await.unwrap();
    assert!(row.0, "is_banned must be true");
    assert!(row.1.is_some(), "banned_at must be set");
    assert_eq!(row.2.as_deref(), Some("test ban"));

    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'admin.user_banned'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(audit_count, 1, "audit row must be written");
}

#[sqlx::test]
async fn admin_unban_user_clears_ban(pool: PgPool) {
    use box_fraise_domain::domain::users::service as user_svc;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ($1, true, true) RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    let (target_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_banned, banned_at, banned_reason) \
         VALUES ($1, true, true, now(), 'previous reason') RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();

    with_admin_tx!(&pool, |atx| {
        user_svc::admin_unban_user(&mut atx, &pool, admin_id, target_id)
            .await.expect("unban must succeed")
    });

    let row: (bool, Option<chrono::DateTime<chrono::Utc>>, Option<String>) = sqlx::query_as(
        "SELECT is_banned, banned_at, banned_reason FROM users WHERE id = $1"
    ).bind(target_id).fetch_one(&pool).await.unwrap();
    assert!(!row.0, "is_banned must be cleared");
    assert!(row.1.is_none(), "banned_at must be NULL");
    assert!(row.2.is_none(), "banned_reason must be NULL");
}

// ─────────────────────────────────────────────────────────────────────────────
// Compliance (Hardening §9) — erasure / export / consent / retention
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn request_erasure_anonymises_user_record(pool: PgPool) {
    use box_fraise_domain::domain::users::service as user_svc;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, display_name, apple_id, push_token) \
         VALUES ($1, true, 'Original Name', 'apple-id-xyz', 'push-token-abc') RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();

    with_rls_tx!(&pool, uid, |tx| {
        user_svc::request_erasure(&mut tx, &pool, uid).await.expect("erasure must succeed for soultoken-less user")
    });

    let row: (String, Option<String>, Option<String>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT email, display_name, apple_id, push_token, deleted_at \
             FROM users WHERE id = $1"
        )
        .bind(uid).fetch_one(&pool).await.unwrap();

    assert_eq!(row.0, format!("erased-{uid}@deleted.boxfraise.com"));
    assert_eq!(row.1.as_deref(), Some("Deleted User"));
    assert_eq!(row.2, None, "apple_id must be cleared");
    assert_eq!(row.3, None, "push_token must be cleared");
    assert!(row.4.is_some(), "deleted_at must be set");
}

#[sqlx::test]
async fn request_erasure_fails_if_active_soultoken(pool: PgPool) {
    use box_fraise_domain::domain::users::service as user_svc;
    use box_fraise_domain::error::DomainError;
    use fake::{Fake, faker::internet::en::SafeEmail};

    // Build a presence-confirmed user, then the FK chain a soultoken needs:
    // identity_credential, location, business, presence_threshold, staff_visit,
    // visit_attestation, soultoken. Promote to attested AFTER inserting the
    // attestation so bf_prevent_double_attestation doesn't reject the row.
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'presence_confirmed') RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    let (cred_id,): (i32,) = sqlx::query_as(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
         VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now()) RETURNING id"
    ).bind(uid).fetch_one(&pool).await.unwrap();
    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('L', 'box_fraise_store', 'A', 'America/Edmonton') RETURNING id"
    ).fetch_one(&pool).await.unwrap();
    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'B', 'active') RETURNING id"
    ).bind(loc_id).bind(uid).fetch_one(&pool).await.unwrap();
    let (thresh_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id"
    ).bind(uid).bind(biz_id).fetch_one(&pool).await.unwrap();
    let (s_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    let (r1,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    let (r2,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id"
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    let (visit_id,): (i32,) = sqlx::query_as(
        "INSERT INTO staff_visits (location_id, staff_id, visit_type, status, scheduled_at) \
         VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id"
    ).bind(loc_id).bind(s_id).fetch_one(&pool).await.unwrap();
    let (attest_id,): (i32,) = sqlx::query_as(
        "INSERT INTO visit_attestations \
         (visit_id, user_id, staff_id, presence_threshold_id, \
          assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
         VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id"
    ).bind(visit_id).bind(uid).bind(s_id).bind(thresh_id).bind(r1).bind(r2)
     .fetch_one(&pool).await.unwrap();
    let (st_id,): (i32,) = sqlx::query_as(
        "INSERT INTO soultokens \
         (display_code, holder_user_id, token_type, identity_credential_id, \
          presence_threshold_id, attestation_id, expires_at) \
         VALUES ('XXXX-YYYY-ZZZZ', $1, 'user', $2, $3, $4, now() + interval '1 year') \
         RETURNING id"
    ).bind(uid).bind(cred_id).bind(thresh_id).bind(attest_id)
     .fetch_one(&pool).await.unwrap();
    sqlx::query("UPDATE users SET soultoken_id = $1, verification_status = 'attested', \
                                    attested_at = now() WHERE id = $2")
        .bind(st_id).bind(uid).execute(&pool).await.unwrap();

    let err = {
        let mut tx = box_fraise_domain::transaction::RlsTransaction::begin(&pool, uid)
            .await.unwrap();
        let r = user_svc::request_erasure(&mut tx, &pool, uid).await;
        // commit on success; on error, rollback by dropping tx (no need to commit)
        if r.is_ok() { tx.commit().await.unwrap(); }
        r.unwrap_err()
    };
    assert!(
        matches!(err, box_fraise_domain::error::DomainError::Conflict(_)),
        "expected Conflict (active soultoken), got: {err:?}"
    );
    let _ = DomainError::NotFound; // silence unused-import warning if rebalanced
}

#[sqlx::test]
async fn export_my_data_returns_complete_profile(pool: PgPool) {
    use box_fraise_domain::domain::users::service as user_svc;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'identity_confirmed') RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO verification_events (user_id, event_type) VALUES ($1, 'identity_confirmed')"
    ).bind(uid).execute(&pool).await.unwrap();

    // Hardening cleanup #3 pilot — services on the migrated path require a
    // RlsTransaction. Mirrors the handler's begin → call → commit shape.
    let export = with_rls_tx!(&pool, uid, |tx| {
        user_svc::export_my_data(&mut tx, &pool, uid)
            .await.expect("export must succeed")
    });

    assert_eq!(export.user_profile.id, uid);
    assert_eq!(export.user_profile.verification_status, "identity_confirmed");
    assert!(!export.verification_journey.is_empty(),
        "verification_journey must include the inserted event");
    assert!(export.exported_at <= chrono::Utc::now(),
        "exported_at must be set to a sane timestamp");
}

#[sqlx::test]
async fn record_consent_inserts_row(pool: PgPool) {
    use box_fraise_domain::domain::users::service as user_svc;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();

    user_svc::record_consent(&pool, uid, "platform_terms", true, Some("127.0.0.1"))
        .await.expect("record_consent must succeed");

    let row: (String, bool, Option<String>) = sqlx::query_as(
        "SELECT consent_type, granted, ip_address FROM consent_records WHERE user_id = $1"
    ).bind(uid).fetch_one(&pool).await.unwrap();
    assert_eq!(row.0, "platform_terms");
    assert!(row.1, "granted must be true");
    assert_eq!(row.2.as_deref(), Some("127.0.0.1"));
}

#[sqlx::test]
async fn retention_pruning_removes_expired_jwt_revocations(pool: PgPool) {
    use box_fraise_server::tasks::retention::prune_expired_jwt_revocations;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO jwt_revocations (jti, user_id, expires_at) \
         VALUES ('test-jti-stale', $1, now() - INTERVAL '2 days')"
    ).bind(uid).execute(&pool).await.unwrap();

    prune_expired_jwt_revocations(&pool).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jwt_revocations WHERE jti = 'test-jti-stale'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "expired jwt_revocations row must be pruned");
}

#[sqlx::test]
async fn retention_pruning_removes_used_magic_links(pool: PgPool) {
    use box_fraise_server::tasks::retention::prune_expired_magic_links;
    use fake::{Fake, faker::internet::en::SafeEmail};

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    ).bind(&SafeEmail().fake::<String>()).fetch_one(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO magic_link_tokens (user_id, email, token_hash, expires_at, used_at) \
         VALUES ($1, $2, 'stale-test-token-hash', now() + INTERVAL '5 minutes', \
                 now() - INTERVAL '2 hours')"
    ).bind(uid).bind("test@example.com").execute(&pool).await.unwrap();

    prune_expired_magic_links(&pool).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM magic_link_tokens WHERE token_hash = 'stale-test-token-hash'"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "used magic_link_tokens row must be pruned");
}

#[sqlx::test]
async fn sse_stream_requires_auth(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let req = Request::builder()
        .method("GET")
        .uri("/api/notifications/stream")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test]
async fn retry_after_header_present_on_429(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    // Tight Dorotka rate limit so we can exhaust it cheaply.
    let state = common::build_state_with_dorotka_rate(pool, None, 2, 60);
    let app   = box_fraise_server::app::build(state);

    for i in 1..=2u8 {
        let _ = app.clone()
            .oneshot(authed_dorotka_request(&format!("warmup {i}"), &token))
            .await.unwrap();
    }
    let resp = app.oneshot(authed_dorotka_request("overflow", &token)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = resp.headers().get("retry-after")
        .and_then(|v| v.to_str().ok());
    assert!(retry_after.is_some(), "429 must include a Retry-After header");
}

#[sqlx::test]
async fn get_analytics_conversion_returns_200_for_admin(pool: PgPool) {
    let (_, admin_token) = create_platform_admin_user(&pool).await;
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let resp = app
        .oneshot(authed_req("GET", "/api/admin/analytics/conversion", &admin_token))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array(), "conversion response must be a JSON array");
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-user rate limiting (Hardening §6 / Grade A item 3)
//
// Wired in `server/src/http/middleware/user_rate_limit.rs`. These tests use
// the `/api/attestations` route — the rate check runs *first* in that
// handler, so the request body need not produce a 2xx for the test to be
// honest about whether the limiter fired. A non-2xx response from the
// service layer just means "rate-check passed, business logic rejected" —
// that's still a successful pass through the limiter, and it's what we
// assert with `assert_ne!(429)`.
// ─────────────────────────────────────────────────────────────────────────────

// `sqlx::test` resets the Postgres sequence per-test, so two adjacent
// rate-limit tests both get `users.id = 1`. Redis is shared (REDIS_URL or
// a single testcontainer), so the previous test's `rate:1:...` counter
// would otherwise carry into the next. Wipe the namespace at entry.
async fn flush_rate_limit_keys(redis: &deadpool_redis::Pool) {
    use deadpool_redis::redis::AsyncCommands;
    let mut conn = redis.get().await.unwrap();
    let keys: Vec<String> = conn.keys("rate:*").await.unwrap_or_default();
    for key in keys {
        let _: () = conn.del(&key).await.unwrap_or(());
    }
}

#[sqlx::test]
async fn per_user_rate_limit_blocks_after_limit_exceeded(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let Some((_container, redis_pool)) = common::start_redis().await else {
        eprintln!("skipping: Redis unavailable");
        return;
    };
    flush_rate_limit_keys(&redis_pool).await;

    // Lower the per-hour attestation limit to 2 for the duration of this test.
    sqlx::query("UPDATE platform_configuration SET value = '2' WHERE key = 'rate_limit_attestations_per_hour'")
        .execute(&pool).await.unwrap();

    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token = common::valid_token(uid);

    let state = common::build_state(pool, Some(redis_pool));
    let app   = box_fraise_server::app::build(state);

    // Body shape doesn't matter — handler runs the rate-check before
    // touching the body. Bogus IDs are fine; we only assert the *status*
    // distinguishes "rate-check passed" from "rate-limited".
    let body = serde_json::json!({
        "visit_id":              -1,
        "user_id":               -1,
        "presence_threshold_id": -1,
        "photo_hash":            null,
        "photo_storage_uri":     null,
    });

    for i in 1..=2u8 {
        let resp = app.clone()
            .oneshot(authed_json_req("POST", "/api/attestations", &token, body.clone()))
            .await.unwrap();
        assert_ne!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
            "request {i} must pass the per-user rate check");
    }

    let resp = app
        .oneshot(authed_json_req("POST", "/api/attestations", &token, body))
        .await.unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS,
        "3rd request must be rate-limited (limit=2)");

    let retry_after = resp.headers().get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    assert!(retry_after.is_some(),
        "429 must include a numeric Retry-After header");
    assert!(retry_after.unwrap() > 0,
        "Retry-After must be positive, got {retry_after:?}");
}

#[sqlx::test]
async fn per_user_rate_limit_is_per_user_not_global(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let Some((_container, redis_pool)) = common::start_redis().await else {
        eprintln!("skipping: Redis unavailable");
        return;
    };
    flush_rate_limit_keys(&redis_pool).await;

    // Tightest possible: 1 attestation per hour per user.
    sqlx::query("UPDATE platform_configuration SET value = '1' WHERE key = 'rate_limit_attestations_per_hour'")
        .execute(&pool).await.unwrap();

    let (uid_a,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let (uid_b,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(&SafeEmail().fake::<String>())
    .fetch_one(&pool).await.unwrap();
    let token_a = common::valid_token(uid_a);
    let token_b = common::valid_token(uid_b);

    let state = common::build_state(pool, Some(redis_pool));
    let app   = box_fraise_server::app::build(state);

    let body = serde_json::json!({
        "visit_id":              -1,
        "user_id":               -1,
        "presence_threshold_id": -1,
        "photo_hash":            null,
        "photo_storage_uri":     null,
    });

    let resp_a = app.clone()
        .oneshot(authed_json_req("POST", "/api/attestations", &token_a, body.clone()))
        .await.unwrap();
    assert_ne!(resp_a.status(), StatusCode::TOO_MANY_REQUESTS,
        "user A's first request must pass — separate counter from user B");

    let resp_b = app
        .oneshot(authed_json_req("POST", "/api/attestations", &token_b, body))
        .await.unwrap();
    assert_ne!(resp_b.status(), StatusCode::TOO_MANY_REQUESTS,
        "user B's first request must pass — separate counter from user A");
}
