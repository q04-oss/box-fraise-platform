//! Shared test fixtures and AppState builder.
//!
//! Usage in each test file:
//!   mod common;
//!   let redis = common::start_redis().await;
//!   let Some((_container, redis_pool)) = redis else { return; };
//!   let state = common::build_state(db_pool, Some(redis_pool));

use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use secrecy::SecretString;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::redis::Redis;

use box_fraise_domain::event_bus::EventBus;
use box_fraise_server::{
    app::AppState,
    auth::new_revoked_tokens,
    config::Config,
    http::middleware::{hmac::new_nonce_cache, rate_limit::RateLimiter},
    types::UserId,
};

// ── AppState ──────────────────────────────────────────────────────────────────

pub fn test_config() -> Config {
    Config {
        database_url:    SecretString::from("postgres://localhost/test".to_string()),
        jwt_secret:               SecretString::from("test-jwt-secret-minimum-32-characters!!".to_string()),
        jwt_secret_previous:      None,
        staff_jwt_secret:         SecretString::from("test-staff-secret-minimum-32-chars!!".to_string()),
        staff_jwt_secret_previous: None,
        stripe_secret_key:     SecretString::from("sk_test_placeholder".to_string()),
        stripe_webhook_secret: SecretString::from("whsec_test_secret_for_handler_tests".to_string()),
        admin_pin:       SecretString::from("testpin1".to_string()),
        chocolatier_pin: SecretString::from("testpin2".to_string()),
        supplier_pin:    SecretString::from("testpin3".to_string()),
        review_pin:      None,
        port:            3001,
        hmac_shared_key: Some(SecretString::from("test-hmac-key-32-bytes-exactly!!".to_string())),
        redis_url:       None,
        apple_team_id:   None,
        apple_key_id:    None,
        apple_client_id: None,
        apple_private_key: None,
        resend_api_key:    None,
        anthropic_api_key: None,
        anthropic_base_url: None,
        cloudinary_cloud_name:  None,
        cloudinary_api_key:     None,
        cloudinary_api_secret:  None,
        square_app_id:                None,
        square_app_secret:            None,
        square_oauth_redirect_url:    None,
        square_token_encryption_key:  None,
        api_base_url:        "http://localhost:3001".to_string(),
        platform_fee_bips:   500,
        operator_email:                   None,
        square_order_webhook_signing_key: None,
        square_order_notification_url:    None,
        app_store_id:                     None,
    }
}

pub fn build_state(db: PgPool, redis: Option<RedisPool>) -> AppState {
    build_state_with_config(db, redis, test_config())
}

/// Build an AppState using a fully custom Config — used by tests that need
/// to override specific fields (e.g., `anthropic_base_url` for Dorotka tests).
pub fn build_state_with_config(db: PgPool, redis: Option<RedisPool>, cfg: Config) -> AppState {
    AppState {
        db,
        cfg:          Arc::new(cfg),
        revoked:      new_revoked_tokens(),
        nonces:       new_nonce_cache(),
        redis,
        rate:         RateLimiter::new(120, 60),
        dorotka_rate: RateLimiter::new(20, 60),
        http:         reqwest::Client::new(),
        event_bus:    EventBus::new(),
    }
}

/// Build an AppState wired to a mock Anthropic server — used by Dorotka tests.
pub fn build_state_with_anthropic(
    db:       PgPool,
    redis:    Option<RedisPool>,
    api_key:  &str,
    base_url: &str,
) -> AppState {
    let mut cfg = test_config();
    cfg.anthropic_api_key  = Some(SecretString::from(api_key.to_string()));
    cfg.anthropic_base_url = Some(base_url.to_string());
    build_state_with_config(db, redis, cfg)
}

/// Build a state with a custom Dorotka rate limit — used by rate limit tests
/// that need a tighter window for speed (e.g. 3/1s instead of 20/60s).
pub fn build_state_with_dorotka_rate(
    db: PgPool,
    redis: Option<RedisPool>,
    max_requests: usize,
    window_secs: u64,
) -> AppState {
    let mut state = build_state(db, redis);
    // Rebuild with the custom rate limiter — we shadow the field after construction
    // because AppState::new is not available in tests.
    AppState {
        dorotka_rate: RateLimiter::new(max_requests, window_secs),
        ..state
    }
}

// ── Redis ─────────────────────────────────────────────────────────────────────

/// Resolves a Redis pool for tests, in priority order:
///
/// 1. `REDIS_URL` env var — used in CI (GitHub Actions Redis service).
///    Returns `(None, pool)` — no container to keep alive.
///
/// 2. Docker testcontainer — used in local dev when Docker is available.
///    Returns `(Some(container), pool)` — caller must hold the container alive.
///
/// 3. Neither available — returns `None`.
///    Caller should `return;` to skip the test gracefully.
pub async fn start_redis() -> Option<(Option<ContainerAsync<Redis>>, RedisPool)> {
    // CI path: use the Redis service wired up by the workflow.
    if let Ok(url) = std::env::var("REDIS_URL") {
        let pool = deadpool_redis::Config::from_url(url)
            .create_pool(Some(deadpool_redis::Runtime::Tokio1))
            .expect("Redis pool from REDIS_URL must be created");
        return Some((None, pool));
    }

    // Local dev path: spin up a container. Returns None if Docker is absent.
    match Redis::default().start().await {
        Ok(container) => {
            let port = container
                .get_host_port_ipv4(6379)
                .await
                .expect("Redis container port must be exposed");
            let url  = format!("redis://127.0.0.1:{port}");
            let pool = deadpool_redis::Config::from_url(url)
                .create_pool(Some(deadpool_redis::Runtime::Tokio1))
                .expect("Redis pool must be created");
            Some((Some(container), pool))
        }
        Err(_) => None,
    }
}

// ── DB fixtures ───────────────────────────────────────────────────────────────

pub struct Usr { pub id: UserId }

pub async fn create_user(pool: &PgPool, email: &str) -> Usr {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email) VALUES ($1) RETURNING id"
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("create_user({email}): {e}"));
    Usr { id: UserId::from(id) }
}

/// Creates a user with `email_verified = true` — required for QR token issuance.
pub async fn create_verified_user(pool: &PgPool, email: &str) -> Usr {
    let u = create_user(pool, email).await;
    sqlx::query("UPDATE users SET email_verified = true WHERE id = $1")
        .bind(i32::from(u.id))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("verify_user: {e}"));
    u
}

/// Creates a user with `verification_status = 'attested'` — required to create businesses.
pub async fn create_attested_user(pool: &PgPool, email: &str) -> Usr {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'attested') RETURNING id"
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("create_attested_user({email}): {e}"));
    Usr { id: UserId::from(id) }
}

// ── JWT helpers ───────────────────────────────────────────────────────────────

/// Sign a valid JWT for `user_id` using the test JWT secret.
pub fn valid_token(user_id: i32) -> String {
    use box_fraise_server::auth::Claims;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let exp = (chrono::Utc::now().timestamp() + 86400 * 90) as usize;
    let claims = Claims {
        user_id: UserId::from(user_id),
        exp,
        jti: uuid::Uuid::new_v4().to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test-jwt-secret-minimum-32-characters!!"),
    )
    .unwrap()
}

/// Sign a JWT for `user_id` that is already expired (exp = 1).
pub fn expired_token(user_id: i32) -> String {
    use box_fraise_server::auth::Claims;
    use jsonwebtoken::{encode, EncodingKey, Header};
    let claims = Claims {
        user_id: UserId::from(user_id),
        exp: 1, // Unix epoch + 1 second — always expired
        jti: "test-expired-jti".to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test-jwt-secret-minimum-32-characters!!"),
    )
    .unwrap()
}
