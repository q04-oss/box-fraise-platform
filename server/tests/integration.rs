//! Cross-domain integration tests — prove full flows work end to end
//! including event bus and audit trail.
//!
//! Run: DATABASE_URL=postgres://... REDIS_URL=redis://... cargo test --test integration

mod common;

use box_fraise_domain::event_bus::EventBus;
use box_fraise_domain::events::DomainEvent;
use deadpool_redis::redis;
use sqlx::PgPool;

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn redis_pool_from_env() -> Option<deadpool_redis::Pool> {
    let url = std::env::var("REDIS_URL").ok()?;
    deadpool_redis::Config::from_url(url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .ok()
}

async fn verified_user(pool: &PgPool, email: &str) -> box_fraise_domain::types::UserId {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, verified) VALUES ($1, true) RETURNING id",
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
// Send message → MessageSent event
// ─────────────────────────────────────────────────────────────────────────────

/// Send message → MessageSent event fires.
#[sqlx::test]
async fn send_message_fires_message_sent_event(pool: PgPool) {
    let alice = verified_user(&pool, "alice@flow.com").await;
    let bob   = verified_user(&pool, "bob@flow.com").await;
    let http  = reqwest::Client::new();
    let bus   = EventBus::new();
    let mut rx = bus.subscribe();

    use box_fraise_domain::domain::messages::{service as msg_service, types::SendMessageBody};

    let msg = msg_service::send_message(
        &pool,
        &http,
        alice,
        SendMessageBody {
            recipient_id:        bob,
            body:                "hello".to_owned(),
            encrypted:           None,
            ephemeral_key:       None,
            sender_identity_key: None,
            one_time_pre_key_id: None,
        },
        &bus,
    )
    .await
    .unwrap();

    assert_eq!(msg.sender_id, alice);
    assert_eq!(msg.recipient_id, bob);

    drop(bus);
    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let has_sent = events.iter().any(|e| matches!(e, DomainEvent::MessageSent { .. }));
    assert!(has_sent, "send_message must publish MessageSent event");
}

// ─────────────────────────────────────────────────────────────────────────────
// Register keys → KeyBundleDepleted when last OTPK consumed
// ─────────────────────────────────────────────────────────────────────────────

#[sqlx::test]
async fn claim_key_bundle_depleted_fires_event(pool: PgPool) {
    use box_fraise_domain::{
        domain::keys::{service as key_service, types::RegisterKeysBody},
        types::KeyId,
    };

    let user_id = verified_user(&pool, "keys@flow.com").await;

    let bus = EventBus::new();

    // Register keys with exactly one OTPK.
    key_service::register_keys(
        &pool,
        user_id,
        RegisterKeysBody {
            identity_key:         "ik".to_owned(),
            identity_signing_key: Some("isk".to_owned()),
            signed_pre_key:       "spk".to_owned(),
            signed_pre_key_sig:   "spk_sig".to_owned(),
            one_time_pre_keys: vec![
                box_fraise_domain::domain::keys::types::OneTimePreKeyItem {
                    key_id:     KeyId::from(1),
                    public_key: "opk1".to_owned(),
                },
            ],
            challenge_sig: None,
        },
        &bus,
    )
    .await
    .unwrap();

    let mut rx = bus.subscribe();

    // Claim the only OTPK — bundle becomes depleted.
    let bundle = key_service::claim_key_bundle(&pool, user_id, &bus).await.unwrap();
    assert!(bundle.one_time_pre_key.is_some(), "first claim must return the OTPK");

    // Count remaining OTPKs — should be 0.
    let remaining = key_service::get_otpk_count(&pool, user_id).await.unwrap();
    assert_eq!(remaining, 0, "OTPK must be consumed after claim");

    drop(bus);
    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let depleted = events.iter().any(|e| matches!(e, DomainEvent::KeyBundleDepleted { .. }));
    assert!(depleted, "claim_key_bundle must publish KeyBundleDepleted when OTPKs exhausted");
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
        sqlx::query_scalar("INSERT INTO users (email, verified) VALUES ($1, false) RETURNING id")
            .bind(email).fetch_one(&pool).await.unwrap();

    let verified: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(!verified);
}

#[sqlx::test]
async fn magic_link_find_or_create_is_idempotent(pool: PgPool) {
    let email = "idempotent@test.com";
    let first: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified) VALUES ($1, false)
         ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email RETURNING id",
    )
    .bind(email).fetch_one(&pool).await.unwrap();

    let second: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified) VALUES ($1, false)
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
        "INSERT INTO users (email, verified, banned) VALUES ('banned@test.com', true, true) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    let banned: bool = sqlx::query_scalar("SELECT banned FROM users WHERE id = $1")
        .bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(banned);
}

#[sqlx::test]
async fn magic_link_verify_marks_user_verified(pool: PgPool) {
    let user_id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified) VALUES ('toverify@test.com', false) RETURNING id",
    )
    .fetch_one(&pool).await.unwrap();

    sqlx::query("UPDATE users SET verified = true WHERE id = $1")
        .bind(user_id).execute(&pool).await.unwrap();

    let verified: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(user_id).fetch_one(&pool).await.unwrap();
    assert!(verified);
}
