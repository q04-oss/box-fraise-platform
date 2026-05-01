//! Integration tests for the loyalty service.
//!
//! Each test gets a fresh Postgres database (sqlx::test runs all migrations),
//! and a dedicated Redis container where needed (testcontainers). No shared
//! mutable state between tests.
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
#[sqlx::test]
async fn qr_token_valid_redemption(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let customer = common::create_verified_user(&pool, "customer@test.com").await;
    let staff    = common::create_user(&pool, "staff@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let qr = loyalty::issue_qr_token(&state, customer.id, biz.id)
        .await
        .expect("token issuance must succeed for a verified user");

    let result = loyalty::stamp_via_qr(&state, staff.id, biz.id, &qr.token, None)
        .await
        .expect("valid stamp must succeed");

    assert_eq!(result.new_balance, 1);
    assert_eq!(result.business_id, biz.id);
    assert!(!result.reward_available, "reward not available until 10 steeps");

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
#[sqlx::test]
async fn qr_token_cross_business_rejected_and_preserved(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let customer   = common::create_verified_user(&pool, "customer@test.com").await;
    let staff_biz2 = common::create_user(&pool, "staff-b@test.com").await;
    let biz1       = common::create_business(&pool, "Biz One").await;
    let biz2       = common::create_business(&pool, "Biz Two").await;
    common::seed_loyalty_config(&pool, biz1.id, 10).await;
    common::seed_loyalty_config(&pool, biz2.id, 10).await;

    let qr = loyalty::issue_qr_token(&state, customer.id, biz1.id)
        .await
        .expect("token issuance must succeed");

    let cross = loyalty::stamp_via_qr(&state, staff_biz2.id, biz2.id, &qr.token, None).await;
    assert!(
        matches!(cross, Err(AppError::Forbidden)),
        "cross-business stamp must return Forbidden, got: {cross:?}"
    );

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events WHERE user_id = $1"
    )
    .bind(i32::from(customer.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "rejected stamp must not record a loyalty event");

    // Token must still be usable by the correct business.
    let staff_biz1 = common::create_user(&pool, "staff-a@test.com").await;
    let ok = loyalty::stamp_via_qr(&state, staff_biz1.id, biz1.id, &qr.token, None).await;
    assert!(
        ok.is_ok(),
        "token must still be usable after cross-business rejection, got: {ok:?}"
    );
}

// ── QR token: single-use enforcement ─────────────────────────────────────────

/// A token consumed on a valid stamp must not be redeemable a second time.
/// This tests the GETDEL atomicity of the Lua consume script.
#[sqlx::test]
async fn qr_token_is_single_use(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let customer = common::create_verified_user(&pool, "customer@test.com").await;
    let staff    = common::create_user(&pool, "staff@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let qr = loyalty::issue_qr_token(&state, customer.id, biz.id)
        .await
        .expect("token issuance must succeed");

    // First redemption succeeds.
    loyalty::stamp_via_qr(&state, staff.id, biz.id, &qr.token, None)
        .await
        .expect("first stamp must succeed");

    // Second attempt with the same token must fail.
    let replay = loyalty::stamp_via_qr(&state, staff.id, biz.id, &qr.token, None).await;
    assert!(
        matches!(replay, Err(AppError::Unauthorized)),
        "replayed token must return Unauthorized, got: {replay:?}"
    );

    // Balance must still be exactly 1 — no double-stamp.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events WHERE user_id = $1 AND event_type = 'steep_earned'"
    )
    .bind(i32::from(customer.id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "single-use token must not produce two events");
}

// ── QR token: unknown token returns Unauthorized ──────────────────────────────

/// A token that was never issued (or has already expired) returns Unauthorized.
#[sqlx::test]
async fn qr_token_unknown_returns_unauthorized(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let staff = common::create_user(&pool, "staff@test.com").await;
    let biz   = common::create_business(&pool, "Test Café").await;

    let result = loyalty::stamp_via_qr(
        &state, staff.id, biz.id,
        "00000000-0000-0000-0000-000000000000", // valid UUID format, not in Redis
        None,
    ).await;

    assert!(
        matches!(result, Err(AppError::Unauthorized)),
        "unknown token must return Unauthorized, got: {result:?}"
    );
}

// ── QR token: unverified user cannot issue ────────────────────────────────────

/// issue_qr_token must be gated behind email verification — an unverified
/// user cannot start earning walk-in steeps.
#[sqlx::test]
async fn qr_token_unverified_user_rejected(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    // create_user (not create_verified_user) — verified = false by default.
    let unverified = common::create_user(&pool, "unverified@test.com").await;
    let biz        = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let result = loyalty::issue_qr_token(&state, unverified.id, biz.id).await;
    assert!(
        matches!(result, Err(AppError::Unprocessable(_))),
        "unverified user must not receive a QR token, got: {result:?}"
    );
}

// ── HTML stamp path: None actor writes NULL to audit ─────────────────────────

/// The HTML /stamp page has no staff JWT — actor_id is None. The audit row
/// must write NULL, not crash or default to 0.
#[sqlx::test]
async fn html_stamp_none_actor_writes_null_to_audit(pool: PgPool) {
    let (_redis, redis_pool) = common::start_redis().await;
    let state = common::build_state(pool.clone(), Some(redis_pool));

    let customer = common::create_verified_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let qr = loyalty::issue_qr_token(&state, customer.id, biz.id)
        .await
        .expect("token issuance must succeed");

    // stamp_via_html passes None for actor — the HTML fallback path.
    loyalty::stamp_via_html(&state, &qr.token, biz.id, None)
        .await
        .expect("HTML stamp must succeed");

    let actor_id: Option<Option<i32>> = sqlx::query_scalar(
        "SELECT actor_id FROM audit_events
         WHERE event_kind = 'loyalty.steep_earned'
         ORDER BY created_at DESC LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert!(
        actor_id.is_some(),           // row exists
        "audit row must be written"
    );
    assert!(
        actor_id.unwrap().is_none(),  // actor_id column is NULL
        "HTML stamp path must write NULL actor_id to audit"
    );
}

// ── Webhook steep: idempotency on duplicate key ───────────────────────────────

/// record_steep_from_webhook must treat a duplicate idempotency_key as a
/// Conflict — not a 500. This exercises the UNIQUE constraint on loyalty_events
/// and confirms the error mapping is correct.
#[sqlx::test]
async fn webhook_steep_idempotent_on_duplicate_key(pool: PgPool) {
    let state = common::build_state(pool.clone(), None); // no Redis needed

    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    let pi_id = "pi_test_idempotency_check";

    // First call succeeds.
    loyalty::record_steep_from_webhook(&state, customer.id, biz.id, pi_id)
        .await
        .expect("first webhook steep must succeed");

    // Second call with the same idempotency key must return Conflict, not 500.
    let replay = loyalty::record_steep_from_webhook(&state, customer.id, biz.id, pi_id).await;
    assert!(
        matches!(replay, Err(AppError::Conflict(_))),
        "duplicate idempotency key must return Conflict, got: {replay:?}"
    );

    // Exactly one event in the ledger despite two calls.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events
         WHERE user_id = $1 AND idempotency_key = $2"
    )
    .bind(i32::from(customer.id))
    .bind(pi_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "duplicate webhook must not produce two events");
}

// ── Append-only ledger: UPDATE and DELETE are rejected by trigger ─────────────

/// The DB trigger must reject any attempt to UPDATE or DELETE a loyalty_event
/// row. This is load-bearing for the integrity of the loyalty ledger.
#[sqlx::test]
async fn loyalty_events_are_append_only(pool: PgPool) {
    let state = common::build_state(pool.clone(), None);

    let customer = common::create_user(&pool, "customer@test.com").await;
    let biz      = common::create_business(&pool, "Test Café").await;
    common::seed_loyalty_config(&pool, biz.id, 10).await;

    loyalty::record_steep_from_webhook(&state, customer.id, biz.id, "pi_append_only_test")
        .await
        .expect("initial event must be recorded");

    let event_id: i64 = sqlx::query_scalar(
        "SELECT id FROM loyalty_events WHERE idempotency_key = 'pi_append_only_test'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // UPDATE must be rejected by the immutability trigger.
    let update = sqlx::query(
        "UPDATE loyalty_events SET event_type = 'reward_redeemed' WHERE id = $1"
    )
    .bind(event_id)
    .execute(&pool)
    .await;
    assert!(
        update.is_err(),
        "UPDATE on loyalty_events must be rejected by trigger"
    );

    // DELETE must also be rejected.
    let delete = sqlx::query("DELETE FROM loyalty_events WHERE id = $1")
        .bind(event_id)
        .execute(&pool)
        .await;
    assert!(
        delete.is_err(),
        "DELETE on loyalty_events must be rejected by trigger"
    );

    // The original event must still be intact.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events WHERE id = $1"
    )
    .bind(event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "event must survive both rejected mutations");
}
