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
    let biz = box_fraise_domain::domain::businesses::service::create_business(
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
    .unwrap();

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
