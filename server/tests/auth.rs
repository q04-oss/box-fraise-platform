//! Integration tests for the auth service.
//!
//! Run with:
//!   DATABASE_URL=postgres://localhost/test cargo test --test auth

mod common;

use box_fraise_server::{domain::auth::service as auth, error::AppError};
use deadpool_redis::redis;
use sqlx::PgPool;

// Redis key constants — must match auth/service.rs exactly.
const VERIFY_PREFIX:      &str = "fraise:email-verify:";
const RESEND_RATE_PREFIX: &str = "fraise:rate:email-resend:";

// ── verify_email: marks user verified and returns email ───────────────────────

/// Seeding a token in Redis and calling verify_email must mark the user as
/// verified in the DB and return their email address.
#[sqlx::test(migrations = "migrations")]
async fn verify_email_marks_user_verified_and_returns_email(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool.clone()));

    let user = common::create_user(&pool, "customer@test.com").await;

    // Confirm user starts unverified.
    let verified_before: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(i32::from(user.id))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!verified_before, "user must start unverified");

    // Seed the verification token directly into Redis.
    let token = "test-verification-token-abc123";
    let key   = format!("{VERIFY_PREFIX}{token}");
    let mut conn = redis_pool.get().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(i32::from(user.id).to_string())
        .arg("EX").arg(86400u64)
        .query_async(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    let email = auth::verify_email(&state, token)
        .await
        .expect("verify_email must succeed with a valid token");

    assert_eq!(email, "customer@test.com");

    let verified_after: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(i32::from(user.id))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(verified_after, "user must be marked verified after token redemption");
}

// ── verify_email: single-use ──────────────────────────────────────────────────

/// A verification token is consumed on first use (GETDEL). A second attempt
/// with the same token must return Unauthorized.
#[sqlx::test(migrations = "migrations")]
async fn verify_email_token_is_single_use(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool.clone()));

    let user  = common::create_user(&pool, "customer@test.com").await;
    let token = "single-use-verify-token-xyz";
    let key   = format!("{VERIFY_PREFIX}{token}");

    let mut conn = redis_pool.get().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&key)
        .arg(i32::from(user.id).to_string())
        .arg("EX").arg(86400u64)
        .query_async(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    // First use succeeds.
    auth::verify_email(&state, token)
        .await
        .expect("first verification must succeed");

    // Second use must fail — token was consumed.
    let replay = auth::verify_email(&state, token).await;
    assert!(
        matches!(replay, Err(AppError::Unauthorized)),
        "replayed verification token must return Unauthorized, got: {replay:?}"
    );
}

// ── verify_email: unknown token ───────────────────────────────────────────────

/// A token that was never issued (or has already expired) returns Unauthorized.
#[sqlx::test(migrations = "migrations")]
async fn verify_email_unknown_token_returns_unauthorized(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let result = auth::verify_email(&state, "00000000-0000-0000-0000-000000000000").await;
    assert!(
        matches!(result, Err(AppError::Unauthorized)),
        "unknown verification token must return Unauthorized, got: {result:?}"
    );
}

// ── resend_verification: rate limit ──────────────────────────────────────────

/// The resend endpoint is rate-limited to one request per 5-minute window.
/// Seeding the counter to 1 (simulating a prior send) and calling again
/// must return Unprocessable — without ever reaching the email-send step.
///
/// Approach: pre-seed the INCR counter to 1 so the next INCR pushes it to 2,
/// which is > 1 and triggers the Unprocessable return before the API-key check.
/// This tests the rate limiting gate in isolation from the email integration.
#[sqlx::test(migrations = "migrations")]
async fn resend_verification_rate_limit_blocks_second_request(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool.clone()));

    let user = common::create_user(&pool, "customer@test.com").await;

    // Simulate "already sent once" — seed the counter to 1.
    let rate_key = format!("{RESEND_RATE_PREFIX}{}", i32::from(user.id));
    let mut conn = redis_pool.get().await.unwrap();
    let _: () = redis::cmd("SET")
        .arg(&rate_key)
        .arg(1u64)
        .arg("EX").arg(300u64)
        .query_async(&mut *conn)
        .await
        .unwrap();
    drop(conn);

    // Next call must be blocked — counter would become 2, which is > 1.
    let result = auth::resend_verification(&state, user.id, "customer@test.com").await;
    assert!(
        matches!(result, Err(AppError::Unprocessable(_))),
        "second resend in window must return Unprocessable, got: {result:?}"
    );

    // Confirm the DB user was not modified (no side effects from a blocked call).
    let verified: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(i32::from(user.id))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!verified, "blocked resend must not alter user state");
}
