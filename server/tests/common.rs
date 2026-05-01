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
    app::{AppState, PinHashes},
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

fn test_pin_hashes() -> Arc<PinHashes> {
    Arc::new(PinHashes {
        admin:       bcrypt::hash("testpin1", 4).unwrap(),
        chocolatier: bcrypt::hash("testpin2", 4).unwrap(),
        supplier:    bcrypt::hash("testpin3", 4).unwrap(),
    })
}

pub fn build_state(db: PgPool, redis: Option<RedisPool>) -> AppState {
    AppState {
        db,
        cfg:          Arc::new(test_config()),
        revoked:      new_revoked_tokens(),
        nonces:       new_nonce_cache(),
        redis,
        rate:         RateLimiter::new(120, 60),
        dorotka_rate: RateLimiter::new(20, 60),
        http:         reqwest::Client::new(),
        pin_hashes:   test_pin_hashes(),
    }
}

/// Build a state with a custom Dorotka rate limit — used by rate limit tests
/// that need a tighter window for speed (e.g. 3/1s instead of 20/60s).
pub fn build_state_with_dorotka_rate(
    db: PgPool,
    redis: Option<RedisPool>,
    max_requests: usize,
    window_secs: u64,
) -> AppState {
    AppState {
        db,
        cfg:          Arc::new(test_config()),
        revoked:      new_revoked_tokens(),
        nonces:       new_nonce_cache(),
        redis,
        rate:         RateLimiter::new(120, 60),
        dorotka_rate: RateLimiter::new(max_requests, window_secs),
        http:         reqwest::Client::new(),
        pin_hashes:   test_pin_hashes(),
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

pub async fn create_location(pool: &PgPool, business_id: i32, name: &str) -> i32 {
    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (business_id, name, address) VALUES ($1, $2, '1 Test St') RETURNING id"
    )
    .bind(business_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("create_location: {e}"));
    id
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

// ── Stripe webhook helpers ─────────────────────────────────────────────────────

/// The test webhook secret — must match the stripe_webhook_secret in test_config().
pub const STRIPE_WEBHOOK_SECRET: &str = "whsec_test_secret_for_handler_tests";

/// Compute a valid `Stripe-Signature` header for a given payload.
/// Mirrors Stripe's HMAC-SHA256 algorithm: `t=<unix>,v1=<hex(HMAC(t.payload))>`.
pub fn sign_stripe_webhook(payload: &[u8]) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let signed = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, STRIPE_WEBHOOK_SECRET.as_bytes());
    let sig = ring::hmac::sign(&key, signed.as_bytes());
    format!("t={},v1={}", timestamp, hex::encode(sig.as_ref()))
}

// ── Square webhook helpers ─────────────────────────────────────────────────────

pub const SQUARE_SIGNING_KEY: &str = "test-square-signing-key-for-integration";
pub const SQUARE_NOTIFICATION_URL: &str =
    "https://test.boxfraise.com/api/webhooks/square/orders";

/// Full Square OAuth config — used by the OAuth callback integration test.
/// Includes app credentials + token encryption key (64 hex chars = 32 bytes).
pub fn test_config_with_square_oauth() -> Config {
    Config {
        square_app_id:               Some("sq0idp-test-app-id".to_string()),
        square_app_secret:           Some(SecretString::from("test-app-secret".to_string())),
        square_oauth_redirect_url:   Some("https://test.example.com/callback".to_string()),
        // 64 hex chars = 32 bytes, required by AES-256-GCM encrypt
        square_token_encryption_key: Some(SecretString::from(
            "0101010101010101010101010101010101010101010101010101010101010101".to_string()
        )),
        ..test_config()
    }
}

pub fn build_state_with_square_oauth(db: PgPool, redis: Option<RedisPool>) -> AppState {
    AppState {
        db,
        cfg:          Arc::new(test_config_with_square_oauth()),
        revoked:      new_revoked_tokens(),
        nonces:       new_nonce_cache(),
        redis,
        rate:         RateLimiter::new(120, 60),
        dorotka_rate: RateLimiter::new(20, 60),
        http:         reqwest::Client::new(),
        pin_hashes:   test_pin_hashes(),
    }
}

/// Seeds encrypted Square OAuth tokens for a business.
/// Uses the same 64-hex encryption key as test_config_with_square_oauth.
/// `expires_at` controls whether load_decrypted will attempt a refresh —
/// pass `Utc::now() + Duration::hours(1)` to trigger the refresh path.
pub async fn seed_oauth_tokens(
    pool:          &PgPool,
    business_id:   i32,
    access_token:  &str,
    refresh_token: &str,
    expires_at:    chrono::DateTime<chrono::Utc>,
) {
    use box_fraise_server::{
        crypto,
        domain::squareoauth::repository::{upsert_tokens, EncryptedTokenRow},
    };
    const ENC_KEY: &str = "0101010101010101010101010101010101010101010101010101010101010101";
    let row = EncryptedTokenRow {
        encrypted_access_token:  crypto::encrypt(ENC_KEY, access_token).unwrap(),
        encrypted_refresh_token: crypto::encrypt(ENC_KEY, refresh_token).unwrap(),
        merchant_id:             "MERCHANT_TEST_123".to_string(),
        square_location_id:      "LOC_TEST_001".to_string(),
        expires_at,
    };
    upsert_tokens(pool, business_id, &row).await.unwrap();
}

/// Seeds a venue_orders row in 'paid' status for testing idempotency paths.
pub async fn seed_paid_venue_order(
    pool:        &PgPool,
    business_id: i32,
    user_id:     UserId,
    pi_id:       &str,
) -> i64 {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO venue_orders
             (user_id, business_id, idempotency_key, stripe_payment_intent_id,
              status, total_cents, platform_fee_cents, notes)
         VALUES ($1, $2, $3, $4, 'paid', 500, 25, '')
         RETURNING id"
    )
    .bind(i32::from(user_id))
    .bind(business_id)
    .bind(format!("idem-test-{pi_id}"))
    .bind(pi_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("seed_paid_venue_order: {e}"));
    id
}

/// Seeds a Square OAuth CSRF state token in Redis.
pub async fn seed_oauth_csrf_state(redis_pool: &RedisPool, token: &str, business_id: i32) {
    use deadpool_redis::redis;
    let key = format!("fraise:square-oauth-state:{token}");
    let mut conn = redis_pool.get().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(business_id.to_string())
        .arg("EX").arg(600u64)
        .query_async(&mut *conn)
        .await
        .unwrap();
}

pub fn test_config_with_square() -> Config {
    Config {
        square_app_id: Some("sq0idp-test".to_string()),
        square_order_webhook_signing_key: Some(
            SecretString::from(SQUARE_SIGNING_KEY.to_string())
        ),
        square_order_notification_url: Some(SQUARE_NOTIFICATION_URL.to_string()),
        ..test_config()
    }
}

pub fn build_state_with_square(db: PgPool, redis: Option<RedisPool>) -> AppState {
    AppState {
        db,
        cfg:          Arc::new(test_config_with_square()),
        revoked:      new_revoked_tokens(),
        nonces:       new_nonce_cache(),
        redis,
        rate:         RateLimiter::new(120, 60),
        dorotka_rate: RateLimiter::new(20, 60),
        http:         reqwest::Client::new(),
        pin_hashes:   test_pin_hashes(),
    }
}

/// Computes the Square webhook signature for a given body.
/// Mirrors Square's algorithm: Base64(HMAC-SHA256(signing_key, url + body)).
pub fn sign_square_payload(key: &str, url: &str, body: &[u8]) -> String {
    use base64::Engine;
    use ring::hmac;
    let k = hmac::Key::new(hmac::HMAC_SHA256, key.as_bytes());
    let mut ctx = hmac::Context::with_key(&k);
    ctx.update(url.as_bytes());
    ctx.update(body);
    base64::engine::general_purpose::STANDARD.encode(ctx.sign().as_ref())
}

// ── Device auth helpers ───────────────────────────────────────────────────────

/// Generate a fresh k256 signing key (Ethereum private key) for device auth tests.
pub fn device_signing_key() -> k256::ecdsa::SigningKey {
    k256::ecdsa::SigningKey::random(&mut rand::thread_rng())
}

/// Derive the Ethereum address (0x-prefixed lowercase hex) from a signing key.
pub fn device_eth_address(signing_key: &k256::ecdsa::SigningKey) -> String {
    use sha3::{Digest, Keccak256};
    let verifying_key = signing_key.verifying_key();
    let encoded = verifying_key.to_encoded_point(false);
    let pubkey_bytes = &encoded.as_bytes()[1..]; // strip 0x04 uncompressed prefix
    let hash: [u8; 32] = Keccak256::digest(pubkey_bytes).into();
    format!("0x{}", hex::encode(&hash[12..]))
}

/// Build the `Authorization: Fraise <address>:<signature>` header value
/// for the current minute — matches the server's ±1 minute tolerance window.
pub fn device_auth_header(signing_key: &k256::ecdsa::SigningKey) -> String {
    use k256::ecdsa::signature::hazmat::PrehashSigner;
    use sha3::{Digest, Keccak256};

    let minute = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() / 60;

    let message = minute.to_string();
    let prefixed = format!("\x19Ethereum Signed Message:\n{}{}", message.len(), message);
    let hash: [u8; 32] = Keccak256::digest(prefixed.as_bytes()).into();

    let (sig, rid): (k256::ecdsa::Signature, k256::ecdsa::RecoveryId) =
        signing_key.sign_prehash(&hash).expect("sign_prehash");

    let mut sig_bytes = [0u8; 65];
    sig_bytes[..64].copy_from_slice(&sig.to_bytes());
    sig_bytes[64] = u8::from(rid) + 27;

    format!("Fraise {}:{}", device_eth_address(signing_key), hex::encode(&sig_bytes))
}

/// Insert a device record in the DB.
/// Creates a throwaway user as the device owner (devices require a user_id).
pub async fn create_device(
    pool:        &PgPool,
    eth_address: &str,
    role:        &str,
    business_id: Option<i32>,
) -> i32 {
    let owner_email = format!("device_owner_{}@test.com", uuid::Uuid::new_v4().simple());
    let (owner_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email) VALUES ($1) RETURNING id"
    )
    .bind(&owner_email)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("create_device owner: {e}"));

    let (id,): (i32,) = sqlx::query_as(
        "INSERT INTO devices (device_address, user_id, role, business_id)
         VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind(eth_address)
    .bind(owner_id)
    .bind(role)
    .bind(business_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("create_device: {e}"));
    id
}

/// Insert a minimal order in 'ready' status for device_collect tests.
/// Returns the nfc_token used for routing.
pub async fn create_ready_order(
    pool:        &PgPool,
    location_id: i32,
    business_id: i32,
) -> String {
    let nfc_token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO orders
             (location_id, business_id, chocolate, finish, quantity,
              total_cents, customer_email, status, nfc_token)
         VALUES ($1, $2, 'guanaja_70', 'plain', 1, 1000, 'customer@test.com',
                 'ready', $3)"
    )
    .bind(location_id)
    .bind(business_id)
    .bind(&nfc_token)
    .execute(pool)
    .await
    .unwrap_or_else(|e| panic!("create_ready_order: {e}"));
    nfc_token
}
