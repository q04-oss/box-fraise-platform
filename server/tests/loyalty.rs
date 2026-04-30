//! Integration tests for the loyalty service.
//!
//! Each test gets a fresh Postgres database (sqlx::test runs all migrations),
//! and a dedicated Redis container (testcontainers). No shared mutable state
//! between tests.
//!
//! Run with:
//!   DATABASE_URL=postgres://localhost/test cargo test --test loyalty

mod common;

use box_fraise_server::{domain::loyalty::service as loyalty, error::AppError};
use sqlx::PgPool;

// ── QR token: valid redemption ────────────────────────────────────────────────

/// Full happy path: customer gets a QR token, staff from the same business
/// stamps it, one loyalty event lands in the DB and the balance is 1.
///
/// This test also exercises the Lua consume script end-to-end against real
/// Redis — confirming that redis::Value::Data matching is correct for this
/// crate version.
#[sqlx::test(migrations = "migrations")]
async fn qr_token_valid_redemption(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let customer = common::create_verified_user(&pool, "customer@test.com").await;
    let staff    = common::create_user(&pool, "staff@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    // Issue QR token — requires email_verified = true.
    let qr = loyalty::issue_qr_token(&state, customer.id, biz.id)
        .await
        .expect("token issuance must succeed for a verified user");

    // Staff from the correct business stamps it.
    let result = loyalty::stamp_via_qr(&state, staff.id, biz.id, &qr.token, None)
        .await
        .expect("valid stamp must succeed");

    assert_eq!(result.new_balance, 1, "balance must be 1 after first stamp");
    assert_eq!(result.business_id, biz.id);
    assert!(!result.reward_available, "reward not available until 10 steeps");

    // Confirm exactly one event is in the append-only ledger.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events
         WHERE user_id = $1 AND business_id = $2 AND event_type = 'steep_earned'"
    )
    .bind(i32::from(customer.id))
    .bind(biz.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "exactly one loyalty event must be recorded");
}

// ── QR token: cross-business rejection preserves the token ───────────────────

/// Security property: a staff member from business B cannot consume a QR token
/// that was issued for business A. The token must survive the rejection so the
/// customer can present it to the correct staff member.
///
/// This is the primary test for the Lua GET→check→DEL script. With the old
/// GETDEL approach the token would be destroyed before the business_id check;
/// the Lua script only DELetes on match.
#[sqlx::test(migrations = "migrations")]
async fn qr_token_cross_business_rejected_and_preserved(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let customer    = common::create_verified_user(&pool, "customer@test.com").await;
    let staff_biz2  = common::create_user(&pool, "staff-b@test.com").await;
    let biz1        = common::create_business(&pool, "Biz One").await;
    let biz2        = common::create_business(&pool, "Biz Two").await;
    common::seed_loyalty_config(&pool, biz1.id, 10).await;
    common::seed_loyalty_config(&pool, biz2.id, 10).await;

    // Token is for biz1.
    let qr = loyalty::issue_qr_token(&state, customer.id, biz1.id)
        .await
        .expect("token issuance must succeed");

    // Staff from biz2 tries to stamp it — must be rejected.
    let cross = loyalty::stamp_via_qr(&state, staff_biz2.id, biz2.id, &qr.token, None).await;
    assert!(
        matches!(cross, Err(AppError::Forbidden)),
        "cross-business stamp must return Forbidden, got: {cross:?}"
    );

    // No loyalty event should have been recorded.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events WHERE user_id = $1"
    )
    .bind(i32::from(customer.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "rejected stamp must not record a loyalty event");

    // The token must still be valid — staff from biz1 can still use it.
    let staff_biz1 = common::create_user(&pool, "staff-a@test.com").await;
    let ok = loyalty::stamp_via_qr(&state, staff_biz1.id, biz1.id, &qr.token, None).await;
    assert!(
        ok.is_ok(),
        "token must still be usable after a cross-business rejection, got: {ok:?}"
    );
}
