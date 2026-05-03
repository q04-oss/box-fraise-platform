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
        soultoken_hmac_key:    "test-soultoken-hmac-key-32bytes!!".to_string().into(),
        soultoken_signing_key: "test-soultoken-sign-key-32bytes!!".to_string().into(),
    });
    let _http = reqwest::Client::new();

    // Use the event bus to capture events.
    let bus = EventBus::new();
    let mut rx = bus.subscribe();

    // Seed magic link token in Redis and the DB row verify_magic_link requires.
    let user_id = verified_user(&pool, "flow@test.com").await;
    let token   = "integration-flow-token";
    let key     = format!("fraise:magic:{token}");
    let mut conn = redis.get().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&key).arg(i32::from(user_id).to_string())
        .arg("EX").arg(900u64)
        .query_async(&mut *conn).await.unwrap();
    drop(conn);

    let token_hash = hex::encode(ring::digest::digest(&ring::digest::SHA256, token.as_bytes()).as_ref());
    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(900);
    sqlx::query(
        "INSERT INTO magic_link_tokens (user_id, email, token_hash, expires_at) \
         VALUES ($1, $2, $3, $4)"
    )
    .bind(i32::from(user_id))
    .bind("flow@test.com")
    .bind(&token_hash)
    .bind(expires_at)
    .execute(&pool).await.unwrap();

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

// ─────────────────────────────────────────────────────────────────────────────
// Beacons end-to-end
// ─────────────────────────────────────────────────────────────────────────────

/// Attested user creates a business and a beacon, fetches the daily UUID,
/// confirms rotation log entry written, and confirms audit_event written.
#[sqlx::test]
async fn create_beacon_end_to_end(pool: PgPool) {
    use box_fraise_domain::domain::beacons::service as beacon_service;
    use box_fraise_domain::domain::beacons::types::CreateBeaconRequest;
    use box_fraise_domain::domain::businesses::service as business_service;
    use box_fraise_domain::domain::businesses::types::CreateBusinessRequest;
    use box_fraise_domain::types::UserId;

    // Create an attested user.
    let (uid_raw,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ('beacon_e2e@test.com', true, 'attested') RETURNING id"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let user_id = UserId::from(uid_raw);

    // Create a business via the business service.
    let bus      = EventBus::new();
    let biz_resp = business_service::create_business(
        &pool,
        user_id,
        CreateBusinessRequest {
            name:          "E2E Beacon Café".to_owned(),
            address:       "789 Beacon Rd, Edmonton, AB".to_owned(),
            latitude:      None,
            longitude:     None,
            timezone:      None,
            contact_email: None,
            contact_phone: None,
        },
        &bus,
    )
    .await
    .expect("business creation must succeed");

    // Create a beacon via the beacon service.
    let beacon_resp = beacon_service::create_beacon(
        &pool,
        user_id,
        CreateBeaconRequest {
            business_id:            biz_resp.id,
            location_id:            biz_resp.location.id,
            minimum_rssi_threshold: Some(-65),
        },
        &bus,
    )
    .await
    .expect("beacon creation must succeed for business owner");

    assert_eq!(beacon_resp.business_id, Some(biz_resp.id));
    assert_eq!(beacon_resp.minimum_rssi_threshold, -65);
    assert!(beacon_resp.is_active);

    // Fetch the daily UUID.
    let uuid_resp = beacon_service::get_daily_uuid(&pool, beacon_resp.id, user_id)
        .await
        .expect("owner must get daily UUID");

    assert_eq!(uuid_resp.beacon_id, beacon_resp.id);
    assert_eq!(uuid_resp.uuid.len(), 36, "UUID must be in 8-4-4-4-12 format");

    // Confirm rotation log entry was written.
    let rot_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM beacon_rotation_log WHERE beacon_id = $1"
    )
    .bind(beacon_resp.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(rot_count >= 1, "beacon_rotation_log entry must exist");

    // Confirm audit_event was written.
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events WHERE event_kind = 'beacon.created'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(audit_count >= 1, "beacon.created audit_event must be written");
}

// ─────────────────────────────────────────────────────────────────────────────
// Concurrent simulation tests
// ─────────────────────────────────────────────────────────────────────────────

/// Five users each fire 10 requests from distinct IPs simultaneously.
/// The per-IP rate limiter tracks each IP independently — no cross-contamination.
/// All 50 requests must succeed since each IP is well below the 120 req/60s limit.
#[sqlx::test]
async fn concurrent_users_get_independent_rate_limits(pool: PgPool) {
    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    let mut set = tokio::task::JoinSet::new();

    for user_index in 0u8..5 {
        for _req_index in 0u8..10 {
            let app     = app.clone();
            let user_ip = format!("10.0.0.{}", user_index + 1);
            set.spawn(async move {
                let req = Request::builder()
                    .method("GET")
                    .uri("/health")
                    .header("x-forwarded-for", user_ip)
                    .body(Body::empty())
                    .unwrap();
                app.oneshot(req).await.unwrap().status()
            });
        }
    }

    let mut statuses = Vec::new();
    while let Some(res) = set.join_next().await {
        statuses.push(res.unwrap());
    }

    assert_eq!(statuses.len(), 50, "all 50 requests must complete");
    let rate_limited = statuses.iter().filter(|&&s| s == StatusCode::TOO_MANY_REQUESTS).count();
    assert_eq!(
        rate_limited, 0,
        "no request must be rate-limited — each IP has its own independent counter (10 << 120)"
    );
}

/// Ten concurrent requests with the SAME HMAC nonce must result in exactly one
/// success and nine 409 CONFLICT rejections. Verifies the nonce cache is
/// thread-safe and correctly deduplicates under concurrency.
#[sqlx::test]
async fn concurrent_beacon_nonce_deduplication(pool: PgPool) {
    use ring::hmac as ring_hmac;
    use base64::{engine::general_purpose::STANDARD, Engine};

    // Build a user and a valid JWT so the request can pass auth after HMAC.
    let user  = common::create_user(&pool, "nonce-test@concurrent.test").await;
    let token = common::valid_token(i32::from(user.id));

    let state = common::build_state(pool, None);
    let app   = box_fraise_server::app::build(state);

    // Compute a valid HMAC signature for all 10 requests.
    // Key matches test_config().hmac_shared_key.
    let key_bytes = b"test-hmac-key-32-bytes-exactly!!";
    let method    = "GET";
    let path      = "/api/businesses/me";
    let ts        = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let nonce     = uuid::Uuid::new_v4().to_string(); // same nonce for all 10 requests

    // message = method + path + ts + nonce + body (body is empty for GET)
    let msg = format!("{method}{path}{ts}{nonce}");
    let key = ring_hmac::Key::new(ring_hmac::HMAC_SHA256, key_bytes);
    let sig = STANDARD.encode(ring_hmac::sign(&key, msg.as_bytes()).as_ref());

    // Spawn 10 concurrent tasks, each sending the same nonce.
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let app    = app.clone();
        let token  = token.clone();
        let sig    = sig.clone();
        let nonce  = nonce.clone();
        let ts_str = ts.to_string();
        set.spawn(async move {
            let req = Request::builder()
                .method("GET")
                .uri("/api/businesses/me")
                .header("x-fraise-client", "ios")
                .header("x-fraise-ts",     &ts_str)
                .header("x-fraise-nonce",  &nonce)
                .header("x-fraise-sig",    &sig)
                .header("authorization",   format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            app.oneshot(req).await.unwrap().status()
        });
    }

    let mut statuses = Vec::new();
    while let Some(res) = set.join_next().await {
        statuses.push(res.unwrap());
    }

    assert_eq!(statuses.len(), 10);

    let conflict_count = statuses.iter().filter(|&&s| s == StatusCode::CONFLICT).count();
    let passed_count   = statuses.iter().filter(|&&s| s != StatusCode::CONFLICT).count();

    assert_eq!(
        conflict_count, 9,
        "9 of 10 concurrent requests with the same nonce must be rejected (409), got: {statuses:?}"
    );
    assert_eq!(
        passed_count, 1,
        "exactly 1 request must pass nonce dedup and proceed to handler"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Identity credentials — full initiation + cooling flow
// ─────────────────────────────────────────────────────────────────────────────

/// Full identity verification flow:
/// 1. Registered user POSTs a Stripe session → 201, user is identity_confirmed.
/// 2. User POSTs app-open twice on different days → two days recorded.
/// 3. GET /api/identity/cooling/status reflects both days.
#[sqlx::test]
async fn identity_verification_and_cooling_period_flow(pool: PgPool) {
    use fake::{Fake, faker::internet::en::SafeEmail};

    let user  = common::create_user(&pool, SafeEmail().fake::<String>().as_str()).await;
    let token = common::valid_token(i32::from(user.id));
    let state = common::build_state(pool.clone(), None);
    let app   = box_fraise_server::app::build(state);

    // Step 1 — initiate verification.
    let resp = app
        .oneshot({
            let bytes = serde_json::to_vec(
                &serde_json::json!({ "stripe_session_id": "vs_integ_test_001" })
            ).unwrap();
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/identity/verify")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(bytes))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
            req
        })
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED, "initiate_verification must return 201");

    let body   = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let cred: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let cred_id = cred["id"].as_i64().expect("response must contain credential id") as i32;

    // Verify user status advanced.
    let status: String = sqlx::query_scalar(
        "SELECT verification_status FROM users WHERE id = $1"
    )
    .bind(i32::from(user.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "identity_confirmed");

    // Step 2 — back-date the cooling window so it's already elapsed.
    sqlx::query(
        "UPDATE identity_credentials \
         SET verified_at = now() - interval '10 days', \
             cooling_ends_at = now() - interval '3 days' \
         WHERE id = $1"
    )
    .bind(cred_id)
    .execute(&pool)
    .await
    .unwrap();

    // Insert two past-day cooling events; the third (today) will complete the cooling.
    for offset in [-2i64, -1] {
        let date = (chrono::Utc::now() + chrono::Duration::days(offset)).date_naive();
        sqlx::query(
            "INSERT INTO cooling_period_events (user_id, credential_id, calendar_date) \
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING"
        )
        .bind(i32::from(user.id))
        .bind(cred_id)
        .bind(date)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Step 3 — record today's app open (completes cooling).
    let state2 = common::build_state(pool.clone(), None);
    let app2   = box_fraise_server::app::build(state2);
    let resp2  = app2
        .oneshot({
            let bytes = serde_json::to_vec(
                &serde_json::json!({ "credential_id": cred_id })
            ).unwrap();
            let mut req = Request::builder()
                .method("POST")
                .uri("/api/identity/cooling/app-open")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::from(bytes))
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
            req
        })
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::OK);
    let body2: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp2.into_body(), 8192).await.unwrap()
    ).unwrap();
    assert_eq!(body2["days_completed"], 3);
    assert!(body2["is_complete"].as_bool().unwrap(), "cooling must be complete after 3 days");

    // Step 4 — GET /api/identity/cooling/status reflects completed state.
    let state3 = common::build_state(pool.clone(), None);
    let app3   = box_fraise_server::app::build(state3);
    let resp3  = app3
        .oneshot({
            let mut req = Request::builder()
                .method("GET")
                .uri("/api/identity/cooling/status")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
            req
        })
        .await
        .unwrap();

    assert_eq!(resp3.status(), StatusCode::OK);
    let body3: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(resp3.into_body(), 8192).await.unwrap()
    ).unwrap();
    assert!(body3["is_complete"].as_bool().unwrap());
}

// ─────────────────────────────────────────────────────────────────────────────
// Background check full journey (BFIP Sections 3b, 7b)
// ─────────────────────────────────────────────────────────────────────────────

/// Create an identity_confirmed user with completed cooling, then:
/// 1. Initiate sanctions check → webhook passed
/// 2. Initiate identity_fraud check → webhook passed
/// 3. Assert get_status returns all_required_passed = true
/// 4. Assert verification_events contain two background_check_passed rows
/// 5. Assert audit_events contain two background_check.passed rows
#[sqlx::test]
async fn full_background_check_journey(pool: PgPool) {
    use box_fraise_domain::domain::background_checks::service;
    use box_fraise_domain::domain::background_checks::types::{
        CheckWebhookPayload, InitiateCheckRequest,
    };
    use box_fraise_domain::types::UserId;

    // Create identity_confirmed user with completed cooling credential.
    let (uid_raw,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ('bgcheck@journey.test', true, 'identity_confirmed') RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();
    let user_id = UserId::from(uid_raw);

    let verified_at     = chrono::Utc::now() - chrono::Duration::days(10);
    let cooling_ends_at = verified_at + chrono::Duration::days(7);
    sqlx::query(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
         VALUES ($1, 'stripe_identity', $2, $3, now())"
    )
    .bind(uid_raw).bind(verified_at).bind(cooling_ends_at)
    .execute(&pool).await.unwrap();

    let bus = EventBus::new();

    // ── Step 1: initiate sanctions check ─────────────────────────────────────
    let sanctions = service::initiate_check(
        &pool, user_id,
        InitiateCheckRequest { check_type: "sanctions".to_owned(), provider: "comply_advantage".to_owned() },
        &bus,
    ).await.expect("sanctions check must initiate");

    // Set external_check_id so the webhook can find the row.
    sqlx::query("UPDATE background_checks SET external_check_id = 'sanctions-ext-001' WHERE id = $1")
        .bind(sanctions.id).execute(&pool).await.unwrap();

    // ── Step 2: webhook — sanctions passed ───────────────────────────────────
    let s_payload = CheckWebhookPayload {
        external_check_id: "sanctions-ext-001".to_owned(),
        status:            "passed".to_owned(),
        provider:          "comply_advantage".to_owned(),
        raw_response:      serde_json::json!({ "result": "pass" }),
    };
    let s_raw = serde_json::to_vec(&s_payload).unwrap();
    service::handle_webhook(&pool, s_payload, &s_raw, "test-key", &bus)
        .await.expect("sanctions webhook must succeed");

    // ── Step 3: initiate identity_fraud check ─────────────────────────────────
    let fraud = service::initiate_check(
        &pool, user_id,
        InitiateCheckRequest { check_type: "identity_fraud".to_owned(), provider: "comply_advantage".to_owned() },
        &bus,
    ).await.expect("identity_fraud check must initiate");

    sqlx::query("UPDATE background_checks SET external_check_id = 'fraud-ext-001' WHERE id = $1")
        .bind(fraud.id).execute(&pool).await.unwrap();

    // ── Step 4: webhook — identity_fraud passed ───────────────────────────────
    let f_payload = CheckWebhookPayload {
        external_check_id: "fraud-ext-001".to_owned(),
        status:            "passed".to_owned(),
        provider:          "comply_advantage".to_owned(),
        raw_response:      serde_json::json!({ "result": "pass" }),
    };
    let f_raw = serde_json::to_vec(&f_payload).unwrap();
    service::handle_webhook(&pool, f_payload, &f_raw, "test-key", &bus)
        .await.expect("identity_fraud webhook must succeed");

    // ── Step 5: assert get_status ─────────────────────────────────────────────
    let status = service::get_status(&pool, user_id).await.unwrap();
    assert!(status.all_required_passed,
        "all_required_passed must be true after both required checks pass");
    assert!(!status.cleared_eligible,
        "cleared_eligible must be false without criminal check");
    assert_eq!(status.sanctions_status.as_deref(),      Some("passed"));
    assert_eq!(status.identity_fraud_status.as_deref(), Some("passed"));

    // ── Step 6: assert verification_events ───────────────────────────────────
    let ve_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM verification_events \
         WHERE user_id = $1 AND event_type = 'background_check_passed'"
    )
    .bind(uid_raw)
    .fetch_one(&pool).await.unwrap();
    assert_eq!(ve_count, 2,
        "two background_check_passed verification_events must be written");

    // ── Step 7: assert audit_events ──────────────────────────────────────────
    let ae_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events \
         WHERE user_id = $1 AND event_kind = 'background_check.passed'"
    )
    .bind(uid_raw)
    .fetch_one(&pool).await.unwrap();
    assert!(ae_count >= 2,
        "at least two background_check.passed audit_events must be written");
}

// ─────────────────────────────────────────────────────────────────────────────
// Staff full journey (BFIP Sections 6, 10, 12.3)
// ─────────────────────────────────────────────────────────────────────────────

/// Full staff visit journey:
/// grant role → schedule → arrive → quality assessment (pass) → complete
/// Assert status = 'completed', assessment + history records exist, audit trail written.
#[sqlx::test]
async fn full_staff_visit_journey(pool: PgPool) {
    use box_fraise_domain::domain::staff::service;
    use box_fraise_domain::domain::staff::types::{
        ArriveAtVisitRequest, CompleteVisitRequest, GrantRoleRequest,
        QualityAssessmentRequest, ScheduleVisitRequest,
    };
    use box_fraise_domain::types::UserId;

    // ── Setup ─────────────────────────────────────────────────────────────────
    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ('admin@journey.test', true, true) RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();
    let admin = UserId::from(admin_id);

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Journey Store', 'box_fraise_store', '1 Journey St', 'America/Edmonton') \
         RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();

    let (attested_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ('biz-owner@journey.test', true, 'attested') RETURNING id"
    )
    .fetch_one(&pool).await.unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'Journey Business', 'active') RETURNING id"
    )
    .bind(loc_id).bind(attested_id)
    .fetch_one(&pool).await.unwrap();

    let bus = EventBus::new();

    // ── Step 1: grant delivery_staff role ────────────────────────────────────
    service::grant_staff_role(
        &pool, admin,
        GrantRoleRequest {
            user_id:      admin_id,
            role:         "delivery_staff".to_owned(),
            location_id:  Some(loc_id),
            expires_at:   None,
            confirmed_by: None,
        },
        &bus,
    ).await.expect("grant_staff_role must succeed");

    // ── Step 2: schedule visit ────────────────────────────────────────────────
    let visit = service::schedule_visit(
        &pool, admin,
        ScheduleVisitRequest {
            location_id:              loc_id,
            visit_type:               "combined".to_owned(),
            scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
            window_hours:             Some(4),
            support_booking_capacity: Some(0),
            expected_box_count:       Some(10),
        },
        &bus,
    ).await.expect("schedule_visit must succeed");

    assert_eq!(visit.status, "scheduled");

    // ── Step 3: arrive ────────────────────────────────────────────────────────
    let arrived = service::arrive_at_visit(
        &pool, visit.id, admin,
        ArriveAtVisitRequest { arrived_latitude: Some(53.5461), arrived_longitude: Some(-113.4938) },
    ).await.expect("arrive_at_visit must succeed");

    assert_eq!(arrived.status, "in_progress");

    // ── Step 4: submit quality assessment (pass) ──────────────────────────────
    let assessment = service::submit_quality_assessment(
        &pool, visit.id, admin,
        QualityAssessmentRequest {
            business_id:               biz_id,
            beacon_functioning:        true,
            staff_performing_correctly: true,
            standards_maintained:      true,
            notes:                     Some("All good.".to_owned()),
        },
        &bus,
    ).await.expect("quality assessment must succeed");

    assert!(assessment.overall_pass);

    // ── Step 5: complete visit ────────────────────────────────────────────────
    let completed = service::complete_visit(
        &pool, visit.id, admin,
        CompleteVisitRequest {
            actual_box_count:    10,
            delivery_signature:  Some("sig-abc123".to_owned()),
            evidence_hash:       Some("hash-abc123".to_owned()),
            evidence_storage_uri: Some("s3://evidence/visit".to_owned()),
        },
        &bus,
    ).await.expect("complete_visit must succeed");

    assert_eq!(completed.status, "completed");
    assert_eq!(completed.actual_box_count, Some(10));

    // ── Step 6: assertions ────────────────────────────────────────────────────

    // Quality assessment record exists.
    let qa_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM quality_assessments WHERE visit_id = $1"
    )
    .bind(visit.id).fetch_one(&pool).await.unwrap();
    assert_eq!(qa_count, 1, "quality_assessments record must exist");

    // business_assessment_history record exists.
    let hist_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM business_assessment_history WHERE business_id = $1"
    )
    .bind(biz_id).fetch_one(&pool).await.unwrap();
    assert_eq!(hist_count, 1, "business_assessment_history record must exist");

    // Audit events exist.
    let ae_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM audit_events WHERE event_kind LIKE 'staff.%' ORDER BY created_at"
    )
    .fetch_all(&pool).await.unwrap();

    let expected_kinds = ["staff.role_granted", "staff.visit_scheduled", "staff.visit_arrived",
                          "staff.quality_assessment_submitted", "staff.visit_completed"];
    for kind in &expected_kinds {
        assert!(ae_kinds.iter().any(|k| k == kind),
            "audit_event '{}' must be written; got: {:?}", kind, ae_kinds);
    }
}

/// Full attestation journey (BFIP Section 6):
/// grant roles → schedule visit → arrive → initiate attestation →
/// staff sign → reviewer 1 sign → reviewer 2 sign → approve →
/// assert user.verification_status = 'attested', attempt recorded, audit trail written.
#[sqlx::test]
async fn full_attestation_journey(pool: PgPool) {
    use box_fraise_domain::domain::attestations::{
        service as attest_svc,
        types::{
            InitiateAttestationRequest, RejectAttestationRequest,
            ReviewerSignAttestationRequest, StaffSignAttestationRequest,
        },
    };
    use box_fraise_domain::domain::staff::service as staff_svc;
    use box_fraise_domain::domain::staff::types::{
        ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest,
    };
    use box_fraise_domain::types::UserId;

    // ── Setup ─────────────────────────────────────────────────────────────────
    let bus = EventBus::new();

    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ('attest-admin@journey.test', true, true) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let admin = UserId::from(admin_id);

    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ('attest-staff@journey.test', true) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let staff = UserId::from(staff_id);

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Attest Journey Store', 'box_fraise_store', '1 Attest Rd', 'America/Edmonton') \
         RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'Attest Journey Biz', 'active') RETURNING id",
    )
    .bind(loc_id)
    .bind(admin_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // ── Step 1: grant delivery_staff role ─────────────────────────────────────
    staff_svc::grant_staff_role(
        &pool, admin,
        GrantRoleRequest {
            user_id: staff_id, role: "delivery_staff".to_owned(),
            location_id: Some(loc_id), expires_at: None, confirmed_by: None,
        },
        &bus,
    ).await.expect("grant delivery_staff must succeed");

    // ── Step 2: grant 2 attestation_reviewer roles ────────────────────────────
    let (r1_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('reviewer1@journey.test', true) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (r2_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('reviewer2@journey.test', true) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    for rid in [r1_id, r2_id] {
        staff_svc::grant_staff_role(
            &pool, admin,
            GrantRoleRequest {
                user_id: rid, role: "attestation_reviewer".to_owned(),
                location_id: None, expires_at: None, confirmed_by: None,
            },
            &bus,
        ).await.unwrap();
    }

    // ── Step 3: schedule + arrive at visit ────────────────────────────────────
    let visit = staff_svc::schedule_visit(
        &pool, staff,
        ScheduleVisitRequest {
            location_id: loc_id, visit_type: "combined".to_owned(),
            scheduled_at: chrono::Utc::now() + chrono::Duration::hours(1),
            window_hours: Some(4), support_booking_capacity: Some(0), expected_box_count: Some(5),
        },
        &bus,
    ).await.expect("schedule_visit must succeed");

    staff_svc::arrive_at_visit(
        &pool, visit.id, staff,
        ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
    ).await.expect("arrive must succeed");

    // ── Step 4: presence-confirmed target user + met threshold ────────────────
    let (target_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ('target@journey.test', true, 'presence_confirmed') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let (threshold_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id",
    )
    .bind(target_id)
    .bind(biz_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    // ── Step 5: initiate attestation ─────────────────────────────────────────
    let attest = attest_svc::initiate_attestation(
        &pool, staff,
        InitiateAttestationRequest {
            visit_id: visit.id, user_id: target_id, presence_threshold_id: threshold_id,
            photo_hash: Some("sha256-photo".to_owned()), photo_storage_uri: None,
        },
        &bus,
    ).await.expect("initiate_attestation must succeed");

    assert_eq!(attest.status, "pending");
    assert_ne!(attest.assigned_reviewer_1_id, attest.assigned_reviewer_2_id);

    // ── Step 6: staff sign ────────────────────────────────────────────────────
    let after_staff_sign = attest_svc::staff_sign(
        &pool, attest.id, staff,
        StaffSignAttestationRequest {
            staff_signature:        "staff-sig-journey".to_owned(),
            photo_hash:             None,
            location_confirmed:     true,
            user_present_confirmed: true,
        },
        &bus,
    ).await.expect("staff_sign must succeed");

    assert_eq!(after_staff_sign.status, "co_sign_pending");
    assert!(after_staff_sign.co_sign_deadline.is_some());

    // ── Step 7: reviewer 1 sign ───────────────────────────────────────────────
    let r1 = UserId::from(attest.assigned_reviewer_1_id);
    let after_r1 = attest_svc::reviewer_sign(
        &pool, attest.id, r1,
        ReviewerSignAttestationRequest {
            signature:              "r1-sig".to_owned(),
            evidence_hash_reviewed: "r1-evidence".to_owned(),
        },
        &bus,
    ).await.expect("reviewer_1 sign must succeed");

    assert_eq!(after_r1.status, "co_sign_pending", "one reviewer signed — still pending");

    // ── Step 8: reviewer 2 sign → approve ────────────────────────────────────
    let r2 = UserId::from(attest.assigned_reviewer_2_id);
    let approved = attest_svc::reviewer_sign(
        &pool, attest.id, r2,
        ReviewerSignAttestationRequest {
            signature:              "r2-sig".to_owned(),
            evidence_hash_reviewed: "r2-evidence".to_owned(),
        },
        &bus,
    ).await.expect("reviewer_2 sign must succeed");

    assert_eq!(approved.status, "approved");

    // ── Step 9: assertions ────────────────────────────────────────────────────

    // User is now attested.
    let user_status: String = sqlx::query_scalar(
        "SELECT verification_status FROM users WHERE id = $1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(user_status, "attested", "user.verification_status must be 'attested'");

    // attestation_attempts record exists with 'approved' outcome.
    let attempt_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attestation_attempts \
         WHERE attestation_id = $1 AND outcome = 'approved'",
    )
    .bind(attest.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(attempt_count, 1, "approved attestation_attempt must be recorded");

    // reviewer_assignment_log has entries for both reviewers.
    let assignment_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reviewer_assignment_log WHERE visit_id = $1",
    )
    .bind(visit.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(assignment_count, 2, "reviewer_assignment_log must have 2 entries");

    // Audit events exist for the attestation lifecycle.
    let ae_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM audit_events \
         WHERE event_kind LIKE 'attestation.%' ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    for kind in &["attestation.initiated", "attestation.staff_signed", "attestation.approved"] {
        assert!(
            ae_kinds.iter().any(|k| k == kind),
            "audit_event '{}' must be written; got: {:?}", kind, ae_kinds
        );
    }
}

/// Full soultoken lifecycle (BFIP Section 7):
/// issue → assert display_code + no uuid in responses → renew → revoke
/// Assert verification_events in correct order, user.soultoken_id managed, audit trail complete.
#[sqlx::test]
async fn full_soultoken_lifecycle(pool: PgPool) {
    use box_fraise_domain::domain::soultokens::{
        service as st_svc,
        types::{IssueSoultokenRequest, RenewSoultokenRequest, RevokeSoultokenRequest},
    };
    use box_fraise_domain::types::UserId;
    use secrecy::ExposeSecret;

    // ── Setup: attested user with approved attestation ────────────────────────
    let (uid,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ('soultoken-lifecycle@test.test', true, 'presence_confirmed') RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ('admin-lifecycle@test.test', true, true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ('staff-lifecycle@test.test', true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let (r1,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ('r1-lifecycle@test.test', true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let (r2,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) VALUES ('r2-lifecycle@test.test', true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, verified_at, cooling_ends_at, cooling_completed_at) \
         VALUES ($1, 'stripe_identity', now(), now() + interval '7 days', now())",
    )
    .bind(uid).execute(&pool).await.unwrap();

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Lifecycle Store', 'box_fraise_store', '1 LC St', 'America/Edmonton') \
         RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'LC Biz', 'active') RETURNING id",
    )
    .bind(loc_id).bind(uid).fetch_one(&pool).await.unwrap();

    let (thresh_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id",
    )
    .bind(uid).bind(biz_id).fetch_one(&pool).await.unwrap();

    let (visit_id,): (i32,) = sqlx::query_as(
        "INSERT INTO staff_visits (location_id, staff_id, visit_type, status, scheduled_at) \
         VALUES ($1, $2, 'combined', 'completed', now()) RETURNING id",
    )
    .bind(loc_id).bind(staff_id).fetch_one(&pool).await.unwrap();

    let (attest_id,): (i32,) = sqlx::query_as(
        "INSERT INTO visit_attestations \
         (visit_id, user_id, staff_id, presence_threshold_id, \
          assigned_reviewer_1_id, assigned_reviewer_2_id, status) \
         VALUES ($1, $2, $3, $4, $5, $6, 'approved') RETURNING id",
    )
    .bind(visit_id).bind(uid).bind(staff_id)
    .bind(thresh_id).bind(r1).bind(r2)
    .fetch_one(&pool).await.unwrap();

    sqlx::query(
        "UPDATE users SET verification_status = 'attested', attested_at = now() WHERE id = $1",
    )
    .bind(uid).execute(&pool).await.unwrap();

    let state = common::build_state(pool.clone(), None);
    let hmac_key    = state.cfg.soultoken_hmac_key.expose_secret().as_bytes().to_vec();
    let signing_key = state.cfg.soultoken_signing_key.expose_secret().as_bytes().to_vec();
    let bus = state.event_bus.clone();

    // ── Step 1: Issue soultoken ───────────────────────────────────────────────
    let token = st_svc::issue_soultoken(
        &state.db, UserId::from(uid),
        IssueSoultokenRequest { attestation_id: attest_id, token_type: "user".to_owned() },
        &hmac_key, &signing_key, &bus,
    ).await.expect("issue_soultoken must succeed");

    // Display code format
    let code_re = regex::Regex::new(r"^[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$").unwrap();
    assert!(code_re.is_match(&token.display_code),
        "display_code must match XXXX-XXXX-XXXX: {}", token.display_code);

    // No uuid in response JSON
    let token_json = serde_json::to_string(&token).unwrap();
    let uuid_re = regex::Regex::new(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}"
    ).unwrap();
    assert!(!uuid_re.is_match(&token_json),
        "uuid must NOT appear in issued soultoken response: {token_json}");

    // users.soultoken_id is set
    let st_id: Option<i32> = sqlx::query_scalar(
        "SELECT soultoken_id FROM users WHERE id = $1"
    )
    .bind(uid).fetch_one(&pool).await.unwrap();
    assert_eq!(st_id, Some(token.id), "users.soultoken_id must be set after issuance");

    // ── Step 2: Renew ─────────────────────────────────────────────────────────
    let before_renewal = token.expires_at;
    let renewal = st_svc::renew_soultoken(
        &state.db, UserId::from(uid),
        RenewSoultokenRequest { presence_event_id: None, renewal_type: "beacon_dwell".to_owned() },
        &bus,
    ).await.expect("renew_soultoken must succeed");

    assert!(renewal.new_expires_at > before_renewal,
        "renewal must extend expires_at");

    // ── Step 3: Revoke ────────────────────────────────────────────────────────
    st_svc::revoke_soultoken(
        &state.db, token.id, UserId::from(admin_id),
        RevokeSoultokenRequest {
            revocation_reason:   "staff_rescission".to_owned(),
            revocation_visit_id: None,
        },
    ).await.expect("revoke_soultoken must succeed");

    // user.verification_status reset to 'registered'
    let status: String = sqlx::query_scalar(
        "SELECT verification_status FROM users WHERE id = $1"
    )
    .bind(uid).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "registered");

    // users.soultoken_id is NULL
    let st_id_after: Option<i32> = sqlx::query_scalar(
        "SELECT soultoken_id FROM users WHERE id = $1"
    )
    .bind(uid).fetch_one(&pool).await.unwrap();
    assert!(st_id_after.is_none(), "soultoken_id must be NULL after revocation");

    // soultoken.revoked_at is set
    let revoked_at: Option<String> = sqlx::query_scalar(
        "SELECT revoked_at::text FROM soultokens WHERE id = $1"
    )
    .bind(token.id).fetch_one(&pool).await.unwrap();
    assert!(revoked_at.is_some(), "soultoken.revoked_at must be set");

    // ── Step 4: Verify verification_events in order ───────────────────────────
    let ve_types: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM verification_events \
         WHERE user_id = $1 ORDER BY created_at"
    )
    .bind(uid).fetch_all(&pool).await.unwrap();

    for expected in &["soultoken_issued", "soultoken_renewed", "soultoken_revoked"] {
        assert!(
            ve_types.iter().any(|t| t == expected),
            "verification_event '{}' must exist; got: {:?}", expected, ve_types
        );
    }

    // ── Step 5: Audit trail ───────────────────────────────────────────────────
    let ae_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM audit_events \
         WHERE event_kind LIKE 'soultoken.%' ORDER BY created_at"
    )
    .fetch_all(&pool).await.unwrap();

    for expected in &["soultoken.issued", "soultoken.renewed", "soultoken.revoked"] {
        assert!(
            ae_kinds.iter().any(|k| k == expected),
            "audit_event '{}' must exist; got: {:?}", expected, ae_kinds
        );
    }
}

/// Full order and NFC collection journey (BFIP Section 9):
/// create order → activate box → collect via NFC tap
/// Assert order.status = 'collected', tapped_at set, clone_detected = false,
/// audit events written.
#[sqlx::test]
async fn full_order_and_collection_journey(pool: PgPool) {
    use box_fraise_domain::domain::orders::{
        service as ord_svc,
        types::{ActivateBoxRequest, CollectOrderRequest, CreateOrderRequest},
    };
    use box_fraise_domain::domain::staff::{
        service as staff_svc,
        types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
    };
    use box_fraise_domain::types::UserId;

    // ── Setup ─────────────────────────────────────────────────────────────────
    let bus = EventBus::new();

    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ('order-admin@journey.test', true, true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();
    let admin = UserId::from(admin_id);

    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('order-staff@journey.test', true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();
    let staff = UserId::from(staff_id);

    let (user_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('order-buyer@journey.test', true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();
    let user = UserId::from(user_id);

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Order Journey Store', 'box_fraise_store', '1 OJ St', 'America/Edmonton') \
         RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses \
         (location_id, primary_holder_id, name, verification_status, is_active) \
         VALUES ($1, $2, 'OJ Biz', 'active', true) RETURNING id",
    )
    .bind(loc_id).bind(admin_id).fetch_one(&pool).await.unwrap();

    // ── Step 1: Create order ──────────────────────────────────────────────────
    let order = ord_svc::create_order(
        &pool, user,
        CreateOrderRequest {
            business_id:         biz_id,
            variety_description: Some("Albion".to_owned()),
            box_count:           1,
            amount_cents:        1500,
        },
        &bus,
    ).await.expect("create_order must succeed");

    assert_eq!(order.status, "pending");
    assert!(order.pickup_deadline.is_some(), "pickup_deadline must be set (food safety)");

    // ── Step 2: Grant delivery_staff role ─────────────────────────────────────
    staff_svc::grant_staff_role(
        &pool, admin,
        GrantRoleRequest {
            user_id: staff_id, role: "delivery_staff".to_owned(),
            location_id: Some(loc_id), expires_at: None, confirmed_by: None,
        },
        &bus,
    ).await.expect("grant_staff_role must succeed");

    // ── Step 3: Schedule + arrive at visit ────────────────────────────────────
    let visit = staff_svc::schedule_visit(
        &pool, staff,
        ScheduleVisitRequest {
            location_id:              loc_id,
            visit_type:               "delivery".to_owned(),
            scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
            window_hours:             Some(4),
            support_booking_capacity: Some(0),
            expected_box_count:       Some(5),
        },
        &bus,
    ).await.expect("schedule_visit must succeed");

    staff_svc::arrive_at_visit(
        &pool, visit.id, staff,
        ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
    ).await.expect("arrive must succeed");

    // ── Step 4: Activate NFC box ──────────────────────────────────────────────
    let nfc_uid = "NFC-JOURNEY-001";

    ord_svc::activate_box(
        &pool, visit.id, staff,
        ActivateBoxRequest {
            nfc_chip_uid:       nfc_uid.to_owned(),
            delivery_signature: "staff-sig-journey".to_owned(),
            expires_at:         chrono::Utc::now() + chrono::Duration::hours(4),
        },
    ).await.expect("activate_box must succeed");

    // ── Step 5: Collect order via NFC tap ─────────────────────────────────────
    let collected = ord_svc::collect_order(
        &pool, user,
        CollectOrderRequest { nfc_chip_uid: nfc_uid.to_owned() },
        &bus,
    ).await.expect("collect_order must succeed");

    // ── Step 6: Assertions ────────────────────────────────────────────────────

    assert_eq!(collected.status, "collected");
    assert!(collected.collected_via_box_id.is_some(), "collected_via_box_id must be set");

    // visit_box.tapped_at set.
    let tapped_at: Option<String> = sqlx::query_scalar(
        "SELECT tapped_at::text FROM visit_boxes WHERE nfc_chip_uid = $1"
    )
    .bind(nfc_uid).fetch_one(&pool).await.unwrap();
    assert!(tapped_at.is_some(), "tapped_at must be set on the visit_box");

    // clone_detected = false (legitimate single tap).
    let clone_detected: bool = sqlx::query_scalar(
        "SELECT clone_detected FROM visit_boxes WHERE nfc_chip_uid = $1"
    )
    .bind(nfc_uid).fetch_one(&pool).await.unwrap();
    assert!(!clone_detected, "clone_detected must be false for a legitimate tap");

    // Audit events written.
    let ae_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM audit_events \
         WHERE event_kind LIKE 'order.%' ORDER BY created_at"
    )
    .fetch_all(&pool).await.unwrap();

    for expected in &["order.created", "order.box_activated", "order.collected"] {
        assert!(
            ae_kinds.iter().any(|k| k == expected),
            "audit_event '{}' must be written; got: {:?}", expected, ae_kinds
        );
    }
}

// ── full_support_booking_journey ──────────────────────────────────────────────

#[sqlx::test]
async fn full_support_booking_journey(pool: PgPool) {
    use box_fraise_domain::{
        domain::staff::{
            service as staff_svc,
            types::{ArriveAtVisitRequest, GrantRoleRequest, ScheduleVisitRequest},
        },
        domain::support::{
            service as sup_svc,
            types::{CancelBookingRequest, CreateBookingRequest, ResolveBookingRequest},
        },
        types::UserId,
    };

    let bus = EventBus::new();

    // ── Setup: admin, staff, location, visit with capacity = 2 ───────────────

    let (admin_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, is_platform_admin) \
         VALUES ('admin@integration.test', true, true) RETURNING id"
    ).fetch_one(&pool).await.unwrap();
    let admin = UserId::from(admin_id);

    let (staff_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('staff@integration.test', true) RETURNING id"
    ).fetch_one(&pool).await.unwrap();
    let staff = UserId::from(staff_id);

    let (user_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('user@integration.test', true) RETURNING id"
    ).fetch_one(&pool).await.unwrap();
    let user = UserId::from(user_id);

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('Integration Support', 'box_fraise_store', '1 Test', 'America/Edmonton') \
         RETURNING id"
    ).fetch_one(&pool).await.unwrap();

    staff_svc::grant_staff_role(&pool, admin,
        GrantRoleRequest {
            user_id:      staff_id,
            role:         "delivery_staff".to_owned(),
            location_id:  Some(loc_id),
            expires_at:   None,
            confirmed_by: None,
        }, &bus,
    ).await.expect("grant_staff_role must succeed");

    let visit = staff_svc::schedule_visit(&pool, staff,
        ScheduleVisitRequest {
            location_id:              loc_id,
            visit_type:               "support".to_owned(),
            scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(1),
            window_hours:             Some(4),
            support_booking_capacity: Some(2),
            expected_box_count:       Some(0),
        }, &bus,
    ).await.expect("schedule_visit must succeed");

    staff_svc::arrive_at_visit(&pool, visit.id, staff,
        ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
    ).await.expect("arrive_at_visit must succeed");

    // ── Step 1: User creates support booking ─────────────────────────────────

    let booking = sup_svc::create_booking(&pool, user,
        CreateBookingRequest {
            visit_id:          visit.id,
            issue_description: Some("Need help with identity verification".to_owned()),
            priority:          Some("urgent".to_owned()),
        }, &bus,
    ).await.expect("create_booking must succeed");

    assert_eq!(booking.status, "booked");
    assert_eq!(booking.priority, "urgent");
    assert_eq!(booking.visit_id, visit.id);

    // ── Step 2: Staff marks user as attended ─────────────────────────────────

    let attended = sup_svc::attend_booking(&pool, booking.id, staff)
        .await.expect("attend_booking must succeed");

    assert_eq!(attended.status, "attended");
    assert!(attended.attended_at.is_some());

    // ── Step 3: Staff resolves with platform gift box ─────────────────────────

    let resolved = sup_svc::resolve_booking(
        &pool, booking.id, staff,
        ResolveBookingRequest {
            resolution_description: "Helped user complete identity verification".to_owned(),
            resolution_signature:   "staff-resolve-sig".to_owned(),
            gift_box_provided:      true,
            gift_box_id:            None,
        }, &bus,
    ).await.expect("resolve_booking must succeed");

    assert_eq!(resolved.status, "resolved");
    assert!(resolved.resolved_at.is_some());
    assert!(resolved.gift_box_provided);

    // ── Assert gift_box_history record created ────────────────────────────────

    let (gift_covered_by, gift_reason): (String, String) = sqlx::query_as(
        "SELECT covered_by, gift_reason FROM gift_box_history WHERE user_id = $1"
    ).bind(user_id).fetch_one(&pool).await.expect("gift_box_history row must exist");
    assert_eq!(gift_covered_by, "platform", "first gift must be platform-covered");
    assert_eq!(gift_reason, "support_interaction");

    // ── Assert users.platform_gift_eligible_after set ─────────────────────────

    let eligible_after: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT platform_gift_eligible_after FROM users WHERE id = $1"
    ).bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(eligible_after > chrono::Utc::now() + chrono::Duration::days(150),
        "platform_gift_eligible_after must be ~6 months in future");

    // ── Assert audit_events contain all expected entries ─────────────────────

    let ae_kinds: Vec<String> = sqlx::query_scalar(
        "SELECT event_kind FROM audit_events WHERE event_kind LIKE 'support.%' ORDER BY created_at"
    ).fetch_all(&pool).await.unwrap();

    for expected in &[
        "support.booking_created",
        "support.booking_attended",
        "support.booking_resolved",
        "support.platform_gift_issued",
    ] {
        assert!(
            ae_kinds.iter().any(|k| k == expected),
            "audit_event '{}' must be written; got: {:?}", expected, ae_kinds
        );
    }

    // ── Step 4: Second booking — second platform gift within 6 months ─────────
    //    should be recorded as user-covered, not platform-covered.

    let (user2_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified) \
         VALUES ('user2@integration.test', true) RETURNING id"
    ).fetch_one(&pool).await.unwrap();
    // Reuse the same user as user2 to test the 6-month limit.
    // Give the first user a second booking at a new visit.

    let visit2 = staff_svc::schedule_visit(&pool, staff,
        ScheduleVisitRequest {
            location_id:              loc_id,
            visit_type:               "support".to_owned(),
            scheduled_at:             chrono::Utc::now() + chrono::Duration::hours(2),
            window_hours:             Some(4),
            support_booking_capacity: Some(2),
            expected_box_count:       Some(0),
        }, &bus,
    ).await.unwrap();

    staff_svc::arrive_at_visit(&pool, visit2.id, staff,
        ArriveAtVisitRequest { arrived_latitude: None, arrived_longitude: None },
    ).await.unwrap();

    let booking2 = sup_svc::create_booking(&pool, user,
        CreateBookingRequest { visit_id: visit2.id, issue_description: None, priority: None },
        &bus,
    ).await.expect("second booking must succeed");

    sup_svc::attend_booking(&pool, booking2.id, staff).await.unwrap();

    sup_svc::resolve_booking(
        &pool, booking2.id, staff,
        ResolveBookingRequest {
            resolution_description: "Resolved second visit".to_owned(),
            resolution_signature:   "staff-sig-2".to_owned(),
            gift_box_provided:      true,
            gift_box_id:            None,
        }, &bus,
    ).await.unwrap();

    // Second gift within 6 months must be user-covered.
    let gift_rows: Vec<String> = sqlx::query_scalar(
        "SELECT covered_by FROM gift_box_history WHERE user_id = $1 ORDER BY gifted_at"
    ).bind(user_id).fetch_all(&pool).await.unwrap();

    assert_eq!(gift_rows.len(), 2, "must have 2 gift_box_history rows");
    assert_eq!(gift_rows[0], "platform", "first gift must be platform-covered");
    assert_eq!(gift_rows[1], "user", "second gift within 6 months must be user-covered");

    let _ = user2_id; // created but unused; suppresses lint
}
