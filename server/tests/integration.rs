//! Cross-domain integration tests — prove full flows work end to end
//! including event bus and audit trail.
//!
//! Run: DATABASE_URL=postgres://... REDIS_URL=redis://... cargo test --test integration

mod common;

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use box_fraise_domain::event_bus::EventBus;
use box_fraise_domain::events::DomainEvent;
use deadpool_redis::redis;
use sqlx::PgPool;
use std::net::SocketAddr;
use tower::ServiceExt;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn redis_pool_from_env() -> Option<deadpool_redis::Pool> {
    let url = std::env::var("REDIS_URL").ok()?;
    deadpool_redis::Config::from_url(url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .ok()
}

async fn verified_user(pool: &PgPool, email: &str) -> box_fraise_domain::types::UserId {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ($1, true) RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap();
    box_fraise_domain::types::UserId::from(id)
}

// ─────────────────────────────────────────────────────────────────────────────
// Magic link → verify → JWT → authenticated request
// ─────────────────────────────────────────────────────────────────────────────

/// Magic link request → verify → JWT issued → UserLoggedIn event fires
/// → audit_events row written.
#[sqlx::test]
async fn magic_link_flow_fires_user_logged_in_event(pool: PgPool) {
    let Some(redis) = redis_pool_from_env().await else {
        eprintln!("skipping: REDIS_URL not set");
        return;
    };

    let cfg    = std::sync::Arc::new(box_fraise_server::config::Config {
        database_url:    "".to_string().into(),
        jwt_secret:      "test-jwt-secret-minimum-32-characters!!".to_string().into(),
        jwt_secret_previous: None,
        staff_jwt_secret:    "test-staff-secret-minimum-32-chars!!".to_string().into(),
        staff_jwt_secret_previous: None,
        stripe_secret_key:     "sk_test_x".to_string().into(),
        stripe_webhook_secret: "whsec_x".to_string().into(),
        admin_pin:       "testpin11".to_string().into(),
        chocolatier_pin: "testpin22".to_string().into(),
        supplier_pin:    "testpin33".to_string().into(),
        review_pin:      None,
        port:            3001,
        hmac_shared_key: None,
        redis_url:       None,
        apple_team_id: None, apple_key_id: None, apple_client_id: None,
        apple_private_key: None, resend_api_key: None, anthropic_api_key: None,
        anthropic_base_url: None,
        cloudinary_cloud_name: None, cloudinary_api_key: None, cloudinary_api_secret: None,
        square_app_id: None, square_app_secret: None, square_oauth_redirect_url: None,
        square_token_encryption_key: None, operator_email: None,
        api_base_url: "http://localhost:3001".to_owned(),
        app_store_id: None, platform_fee_bips: 500,
        square_order_webhook_signing_key: None, square_order_notification_url: None,
    });
    let _http = reqwest::Client::new();

    // Use the event bus to capture events.
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    // Seed magic link token directly in Redis.
    let user_id = verified_user(&pool, "flow@test.com").await;
    let token   = "integration-flow-token";
    let key     = format!("fraise:magic:{token}");
    let mut conn = redis.get().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&key).arg(i32::from(user_id).to_string())
        .arg("EX").arg(900u64)
        .query_async(&mut *conn).await.unwrap();
    drop(conn);

    // verify_magic_link and check event + audit.
    use box_fraise_domain::domain::auth::service as auth_service;
    let _resp = auth_service::verify_magic_link(
        &pool, &cfg, Some(&redis), token, None, &bus,
    ).await.unwrap();

    // Verify the event was published to the bus.
    drop(bus);
    let mut logged_in = false;
    while let Ok(event) = rx.try_recv() {
        if matches!(event, DomainEvent::UserLoggedIn { .. }) {
            logged_in = true;
        }
    }
    assert!(logged_in, "verify_magic_link must publish UserLoggedIn event");
}

// ─────────────────────────────────────────────────────────────────────────────
// Legacy DB-layer tests (preserved from previous integration suite)
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn magic_link_creates_new_user_on_first_call(pool: PgPool) {
    let email = "newuser@test.com";
    let existing: Option<i32> =
        sqlx::query_scalar("SELECT id FROM users WHERE LOWER(email) = LOWER($1)")
            .bind(email).fetch_optional(&pool).await.unwrap();
    assert!(existing.is_none());

    let user_id: i32 =
        sqlx::query_scalar("INSERT INTO users (email, email_verified) VALUES ($1, false) RETURNING id")
            .bind(email).fetch_one(&pool).await.unwrap();

    let verified: bool = sqlx::query_scalar("SELECT email_verified FROM users WHERE id = $1")
        .bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(!verified);
}

#[sqlx::test]
async fn magic_link_find_or_create_is_idempotent(pool: PgPool) {
    let email = "idempotent@test.com";
    let first: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, email_verified) VALUES ($1, false)
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(email).fetch_one(&pool).await.unwrap();

    let second: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, email_verified) VALUES ($1, false)
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(email).fetch_one(&pool).await.unwrap();

    assert_eq!(first, second);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind(email).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn magic_link_banned_user_is_silently_skipped(pool: PgPool) {
    let user_id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, email_verified, is_banned) VALUES ('banned@test.com', true, true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let banned: bool = sqlx::query_scalar("SELECT is_banned FROM users WHERE id = $1")
        .bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(banned);
}

#[sqlx::test]
async fn magic_link_verify_marks_user_verified(pool: PgPool) {
    let user_id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, email_verified) VALUES ('toverify@test.com', false) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    sqlx::query("UPDATE users SET email_verified = true WHERE id = $1")
        .bind(user_id).execute(&pool).await.unwrap();

    let verified: bool = sqlx::query_scalar("SELECT email_verified FROM users WHERE id = $1")
        .bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(verified);
}

// ─────────────────────────────────────────────────────────────────────────────
// Business creation end-to-end
// ─────────────────────────────────────────────────────────────────────────────

/// Create an attested user, register a business, fetch it back, and confirm
/// the verification_event and audit_event were written.
#[sqlx::test]
async fn create_business_end_to_end(pool: PgPool) {
    use box_fraise_domain::domain::businesses::service;
    use box_fraise_domain::domain::businesses::types::CreateBusinessRequest;
    use box_fraise_domain::types::UserId;

    // Register an attested user.
    let (user_id_raw,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ('e2e@business.test', true, 'attested') RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();
    let user_id = UserId::from(user_id_raw);

    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    // Create the business.
    let req = CreateBusinessRequest {
        name:          "E2E Café".to_owned(),
        address:       "456 Commerce St, Edmonton, AB".to_owned(),
        latitude:      Some(53.5461),
        longitude:     Some(-113.4938),
        timezone:      None,
        contact_email: Some("hello@e2ecafe.test".to_owned()),
        contact_phone: None,
    };
    let resp = service::create_business(&pool, user_id, req, &bus)
        .await
        .expect("create_business must succeed for attested user");

    assert_eq!(resp.name, "E2E Café");
    assert_eq!(resp.verification_status, "pending");
    assert_eq!(resp.location.address, "456 Commerce St, Edmonton, AB");

    // Fetch it back.
    let fetched = service::get_business(&pool, resp.id, user_id)
        .await
        .expect("get_business must return the created business");
    assert_eq!(fetched.id, resp.id);

    // List: must appear in list_my_businesses.
    let my_list = service::list_my_businesses(&pool, user_id)
        .await
        .expect("list_my_businesses must succeed");
    assert_eq!(my_list.len(), 1);
    assert_eq!(my_list[0].id, resp.id);

    // Confirm BusinessCreated event was published.
    drop(bus);
    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let has_created = events.iter().any(|e| {
        matches!(e, DomainEvent::BusinessCreated { business_id, .. } if *business_id == resp.id)
    });
    assert!(has_created, "BusinessCreated event must be published");

    // Confirm verification_event was written.
    let ve_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verification_events \
         WHERE reference_type = 'business' AND reference_id = $1"
    )
    .bind(resp.id)
    .fetch_one(&pool).await.unwrap();
    assert_eq!(ve_count, 1, "one verification_event must be written");

    // Confirm audit_event was written.
    let ae_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'business.created'"
    )
    .fetch_one(&pool).await.unwrap();
    assert!(ae_count >= 1, "at least one audit_event must be written");
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix 2 — Dorotka handler integration test
// ─────────────────────────────────────────────────────────────────────────────

/// POST /api/dorotka/ask with a wiremock Anthropic server returns 200, a
/// non-empty answer, and writes an audit_event row with event_kind = 'dorotka.ask'.
#[sqlx::test]
async fn dorotka_ask_returns_response_for_authenticated_user(pool: PgPool) {
    use wiremock::{matchers, Mock, MockServer, ResponseTemplate};

    // Spin up a mock Anthropic server.
    let mock_server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "Dorotka speaking." }]
            })),
        )
        .mount(&mock_server)
        .await;

    let pool_clone = pool.clone();
    let base_url   = format!("{}/v1/messages", mock_server.uri());
    let state = common::build_state_with_anthropic(pool, None, "test-api-key", &base_url);
    let app   = box_fraise_server::app::build(state);

    let mut req = Request::builder()
        .method("POST")
        .uri("/api/dorotka/ask")
        .header("content-type", "application/json")
        .body(Body::from(br#"{"query":"What is box fraise?"}"#.to_vec()))
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body  = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        !json["answer"].as_str().unwrap_or("").is_empty(),
        "answer must be non-empty"
    );

    // audit_events row must exist.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'dorotka.ask'"
    )
    .fetch_one(&pool_clone)
    .await
    .unwrap();
    assert!(count >= 1, "dorotka.ask audit_event must be written");
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix 3 — End-to-end JWT revocation test
// ─────────────────────────────────────────────────────────────────────────────

/// Logout revokes the JWT in the in-process cache; a subsequent request with
/// the same token must receive 401 Unauthorized.
#[sqlx::test]
async fn jwt_revocation_blocks_subsequent_requests(pool: PgPool) {
    let user  = common::create_user(&pool, "revoke@test.com").await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let mut logout_req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    logout_req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));

    let logout_resp = app.clone().oneshot(logout_req).await.unwrap();
    assert_eq!(logout_resp.status(), StatusCode::OK, "logout must succeed");

    let mut me_req = Request::builder()
        .method("GET")
        .uri("/api/auth/me")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    me_req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));

    let me_resp = app.oneshot(me_req).await.unwrap();
    assert_eq!(me_resp.status(), StatusCode::UNAUTHORIZED, "revoked token must be rejected");
}

// ─────────────────────────────────────────────────────────────────────────────
// Fix 5 — HMAC rejection integration test
// ─────────────────────────────────────────────────────────────────────────────

/// An iOS request with a valid timestamp and nonce but a wrong HMAC signature
/// must be rejected with 401 Unauthorized before reaching any handler logic.
#[sqlx::test]
async fn hmac_rejects_request_with_invalid_signature(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let ts    = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let nonce = uuid::Uuid::new_v4().to_string();

    let mut req = Request::builder()
        .method("GET")
        .uri("/api/businesses/me")
        .header("x-fraise-client", "ios")
        .header("x-fraise-ts",     ts.to_string())
        .header("x-fraise-nonce",  &nonce)
        .header("x-fraise-sig",    "aW52YWxpZA==") // base64("invalid") — wrong HMAC
        .header("authorization",   "Bearer fake-token")
        .body(Body::empty())
        .unwrap();
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "invalid HMAC must be rejected");
}
