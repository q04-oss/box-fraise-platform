//! Shared test fixtures and AppState builder.
//!
//! Usage in each test file:
//!   mod common;
//!   let (redis, pool) = common::start_redis().await;
//!   let state = common::build_state(db_pool, Some(redis_pool));

use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use secrecy::SecretString;
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::redis::Redis;

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
        database_url:    SecretString::from("postgres://localhost/test"),
        jwt_secret:      SecretString::from("test-jwt-secret-minimum-32-characters!!"),
        staff_jwt_secret: SecretString::from("test-staff-secret-minimum-32-chars!!"),
        stripe_secret_key:     SecretString::from("sk_test_placeholder"),
        stripe_webhook_secret: SecretString::from("whsec_placeholder"),
        admin_pin:       SecretString::from("testpin1"),
        chocolatier_pin: SecretString::from("testpin2"),
        supplier_pin:    SecretString::from("testpin3"),
        review_pin:      None,
        port:            3001,
        hmac_shared_key: Some(SecretString::from("test-hmac-key-32-bytes-exactly!!")),
        redis_url:       None,
        apple_team_id:   None,
        apple_key_id:    None,
        apple_client_id: None,
        apple_private_key: None,
        resend_api_key:    None,
        anthropic_api_key: None,
        cloudinary_cloud_name:  None,
        cloudinary_api_key:     None,
        cloudinary_api_secret:  None,
        square_app_id:                None,
        square_app_secret:            None,
        square_oauth_redirect_url:    None,
        square_token_encryption_key:  None,
        api_base_url:        "http://localhost:3001".to_string(),
        platform_fee_bips:   500,
        square_order_webhook_signing_key: None,
        square_order_notification_url:    None,
    }
}

pub fn build_state(db: PgPool, redis: Option<RedisPool>) -> AppState {
    AppState {
        db,
        cfg:     Arc::new(test_config()),
        revoked: new_revoked_tokens(),
        nonces:  new_nonce_cache(),
        redis,
        rate:    RateLimiter::new(),
        http:    reqwest::Client::new(),
    }
}

// ── Redis container ───────────────────────────────────────────────────────────

/// Starts a Redis container and returns (container, pool).
/// The container must be held alive by the caller — dropping it stops Redis.
pub async fn start_redis() -> (ContainerAsync<Redis>, RedisPool) {
    let container = Redis::default()
        .start()
        .await
        .expect("Redis test container must start");
    let port = container
        .get_host_port_ipv4(6379)
        .await
        .expect("Redis port must be exposed");
    let url  = format!("redis://127.0.0.1:{port}");
    let pool = deadpool_redis::Config::from_url(url)
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("Redis pool must be created");
    (container, pool)
}

// ── DB fixtures ───────────────────────────────────────────────────────────────

pub struct Usr { pub id: UserId }
pub struct Biz { pub id: i32   }

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

/// Creates a user with `verified = true` — required for QR token issuance.
pub async fn create_verified_user(pool: &PgPool, email: &str) -> Usr {
    let u = create_user(pool, email).await;
    sqlx::query("UPDATE users SET verified = true WHERE id = $1")
        .bind(i32::from(u.id))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("verify_user: {e}"));
    u
}

pub async fn create_business(pool: &PgPool, name: &str) -> Biz {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (name, type, address, city, launched_at)
         VALUES ($1, 'cafe', '1 Test St', 'Montreal', now())
         RETURNING id"
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("create_business({name}): {e}"));
    Biz { id }
}

pub async fn seed_loyalty_config(pool: &PgPool, business_id: i32, steeps_per_reward: i32) {
    sqlx::query(
        "INSERT INTO business_loyalty_config (business_id, steeps_per_reward)
         VALUES ($1, $2)
         ON CONFLICT (business_id) DO UPDATE SET steeps_per_reward = EXCLUDED.steeps_per_reward"
    )
    .bind(business_id)
    .bind(steeps_per_reward)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("seed_loyalty_config: {e}"));
}
