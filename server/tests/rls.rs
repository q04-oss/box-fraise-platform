//! RLS policy enforcement tests (Hardening cleanup task 3).
//!
//! Every other test suite in this repo runs as the `fraise` superuser,
//! which has `BYPASSRLS` — RLS policies are wired but never fire. This
//! suite proves they actually fire when the connecting role is
//! non-superuser, by impersonating `app_user` via `SET ROLE`.
//!
//! ## How `SET ROLE` works here
//!
//! `SET ROLE` switches the *current* role for a session/transaction. Per
//! Postgres docs, `BYPASSRLS` is checked against the current role, so
//! switching from `fraise` (superuser, BYPASSRLS) to `app_user`
//! (non-superuser, no BYPASSRLS) makes RLS policies enforce.
//!
//! Prerequisite: the connecting role must be a member of the target role.
//! `fraise` is not granted membership by default, so `setup_grants`
//! does so idempotently. Role membership is cluster-level and persists
//! across `#[sqlx::test]` invocations.

use sqlx::PgPool;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Grant `fraise` (the test connection role) membership in `app_user` and
/// `app_admin`. Required so `SET ROLE app_user / app_admin` can succeed.
/// Idempotent — Postgres treats re-grants as no-ops.
async fn setup_grants(pool: &PgPool) {
    let _ = sqlx::query("GRANT app_user TO CURRENT_USER")
        .execute(pool)
        .await;
    let _ = sqlx::query("GRANT app_admin TO CURRENT_USER")
        .execute(pool)
        .await;
}

/// Insert two test users via the superuser pool (bypasses RLS for setup).
/// Returns `(user_a, user_b)` ids.
async fn create_two_users(pool: &PgPool) -> (i32, i32) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let user_a: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'registered') RETURNING id",
    )
    .bind(SafeEmail().fake::<String>())
    .fetch_one(pool)
    .await
    .expect("insert user_a");

    let user_b: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'registered') RETURNING id",
    )
    .bind(SafeEmail().fake::<String>())
    .fetch_one(pool)
    .await
    .expect("insert user_b");

    (user_a, user_b)
}

/// Macro: open a transaction, switch to `app_user` and pin `app.user_id`
/// for the duration, run `$body` against the transaction's connection,
/// then roll the transaction back.
///
/// **Why a transaction.** `set_config(..., true)` is per-transaction. Run
/// outside an explicit `BEGIN`, each statement is its own implicit tx and
/// the setting evaporates the moment that tx commits — by the time the
/// body's SELECT runs, `app.user_id` is empty and the policy's `::integer`
/// cast crashes with `invalid input syntax for type integer: ""`. Wrapping
/// in `pool.begin()` keeps the tx open across SET ROLE, set_config, and
/// the body, so the setting is live throughout. Rollback at the end is a
/// safe default — these tests don't need writes to persist.
///
/// **Closure-style binding** because Rust's `macro_rules!` hygiene
/// prevents internally `let`-bound identifiers from being visible in
/// user-passed expressions (same constraint as `with_rls_tx!` in
/// `domain::transaction`).
macro_rules! as_app_user {
    ($pool:expr, $user_id:expr, |$conn:ident| $body:block) => {{
        let mut tx = $pool.begin().await.expect("begin tx");
        sqlx::query("SET LOCAL ROLE app_user")
            .execute(&mut *tx)
            .await
            .expect("SET LOCAL ROLE app_user — fraise must be a member of app_user");
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(($user_id).to_string())
            .execute(&mut *tx)
            .await
            .expect("set_config app.user_id");
        let $conn = &mut *tx;
        let result = { $body };
        let _ = tx.rollback().await;
        result
    }};
}

/// Same as `as_app_user!` but switches to `app_admin`. Admin policies use
/// `USING (true)` so row visibility is unrestricted; no `app.user_id`
/// setting is required.
macro_rules! as_app_admin {
    ($pool:expr, |$conn:ident| $body:block) => {{
        let mut tx = $pool.begin().await.expect("begin tx");
        sqlx::query("SET LOCAL ROLE app_admin")
            .execute(&mut *tx)
            .await
            .expect("SET LOCAL ROLE app_admin — fraise must be a member of app_admin");
        let $conn = &mut *tx;
        let result = { $body };
        let _ = tx.rollback().await;
        result
    }};
}

// ── Tests ───────────────────────────────────────────────────────────────────

/// Smoke test — confirm `SET ROLE app_user` succeeds (membership grant
/// works) and that `current_user` flips to `app_user` inside the block.
#[sqlx::test(migrations = "../server/migrations")]
async fn set_role_app_user_succeeds_and_drops_superuser(pool: PgPool) {
    setup_grants(&pool).await;
    let role: String = as_app_user!(&pool, 0_i32, |conn| {
        sqlx::query_scalar("SELECT current_user::text")
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    });
    assert_eq!(role, "app_user", "SET ROLE app_user must flip current_user");
}

/// RLS denies cross-user reads on `verification_events` to `app_user`.
/// The policy is `user_id = current_setting('app.user_id')`; with user_a's
/// id set, querying for user_b's rows must return 0.
#[sqlx::test(migrations = "../server/migrations")]
async fn user_cannot_see_another_users_verification_events(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, user_b) = create_two_users(&pool).await;

    sqlx::query(
        "INSERT INTO verification_events (user_id, event_type, metadata) \
         VALUES ($1, 'identity_confirmed', '{}'::jsonb)",
    )
    .bind(user_b)
    .execute(&pool)
    .await
    .expect("insert verification_event for user_b as superuser");

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events WHERE user_id = $1",
        )
        .bind(user_b)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    });

    assert_eq!(
        count, 0,
        "user_a must not see user_b's verification_events under RLS",
    );
}

/// Sanity check the inverse — user_a CAN see their own rows. Without this,
/// the previous test's "0" might just mean RLS is broken closed.
#[sqlx::test(migrations = "../server/migrations")]
async fn user_can_see_own_verification_events(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, _user_b) = create_two_users(&pool).await;

    sqlx::query(
        "INSERT INTO verification_events (user_id, event_type, metadata) \
         VALUES ($1, 'identity_confirmed', '{}'::jsonb)",
    )
    .bind(user_a)
    .execute(&pool)
    .await
    .expect("insert verification_event for user_a as superuser");

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events WHERE user_id = $1",
        )
        .bind(user_a)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    });

    assert_eq!(count, 1, "user_a must see their own verification_events");
}

/// Stronger isolation proof: a SELECT *without* a `WHERE user_id = ...`
/// filter on `verification_events` returns only the caller's own rows.
/// This catches the failure mode where a service forgets the `WHERE` —
/// RLS still scopes the result.
///
/// (Originally targeted `soultokens` for variety, but that table has 4
/// CHECK constraints on `token_type='user'` rows requiring an
/// attestation/credential/threshold chain. The point is to prove RLS
/// enforces; using a second table with simpler constraints keeps the
/// test about the policy, not the seeding.)
#[sqlx::test(migrations = "../server/migrations")]
async fn rls_filters_unscoped_query_to_caller_only(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, user_b) = create_two_users(&pool).await;

    // 3 events for user_a, 5 for user_b — totals are arbitrary; the
    // assertion is on the per-user count seen under RLS.
    for _ in 0..3 {
        sqlx::query(
            "INSERT INTO verification_events (user_id, event_type, metadata) \
             VALUES ($1, 'identity_confirmed', '{}'::jsonb)",
        )
        .bind(user_a)
        .execute(&pool)
        .await
        .expect("seed user_a event");
    }
    for _ in 0..5 {
        sqlx::query(
            "INSERT INTO verification_events (user_id, event_type, metadata) \
             VALUES ($1, 'identity_confirmed', '{}'::jsonb)",
        )
        .bind(user_b)
        .execute(&pool)
        .await
        .expect("seed user_b event");
    }

    // No `WHERE` clause — relies entirely on the RLS policy to scope.
    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar("SELECT COUNT(*) FROM verification_events")
            .fetch_one(&mut *conn)
            .await
            .unwrap()
    });

    assert_eq!(
        count, 3,
        "RLS must hide user_b's 5 rows even from an unscoped SELECT — \
         caller saw {count} rows instead of 3",
    );
}

/// Append-only invariant: `app_user` cannot UPDATE rows in
/// `verification_events`, even their own. Two layers of defence:
/// (a) the `bf_prevent_modification` trigger rejects the update, and
/// (b) `app_user` is not granted UPDATE on append-only tables (revoked by
/// migration 002). Either layer alone is sufficient — the test simply
/// requires the UPDATE not to land.
#[sqlx::test(migrations = "../server/migrations")]
async fn verification_events_rejects_update_as_app_user(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, _) = create_two_users(&pool).await;

    let event_id: i32 = sqlx::query_scalar(
        "INSERT INTO verification_events (user_id, event_type, metadata) \
         VALUES ($1, 'identity_confirmed', '{}'::jsonb) RETURNING id",
    )
    .bind(user_a)
    .fetch_one(&pool)
    .await
    .expect("insert event_id for user_a");

    // Attempt UPDATE as app_user. Either an error or 0 rows_affected
    // satisfies the invariant — both prove the change did not land.
    let outcome: Result<u64, sqlx::Error> = as_app_user!(&pool, user_a, |conn| {
        sqlx::query("UPDATE verification_events SET event_type = $1 WHERE id = $2")
            .bind("attestation_initiated") // any other valid kind
            .bind(event_id)
            .execute(&mut *conn)
            .await
            .map(|r| r.rows_affected())
    });

    let landed_outcome = match outcome {
        Err(_) => "UPDATE rejected with an error",
        Ok(0)  => "UPDATE matched 0 rows",
        Ok(n)  => panic!("UPDATE on append-only table affected {n} rows — invariant violated"),
    };

    // Belt-and-suspenders: confirm the row is still its original value.
    let event_type: String = sqlx::query_scalar(
        "SELECT event_type FROM verification_events WHERE id = $1",
    )
    .bind(event_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        event_type, "identity_confirmed",
        "row's event_type changed despite '{}' — append-only broken",
        landed_outcome,
    );
}

/// `app_admin` has the bypass policy `USING (true)` on every RLS-enabled
/// table — they see all users' rows.
#[sqlx::test(migrations = "../server/migrations")]
async fn admin_can_see_all_users_data(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, user_b) = create_two_users(&pool).await;

    sqlx::query(
        "INSERT INTO verification_events (user_id, event_type, metadata) \
         VALUES ($1, 'identity_confirmed', '{}'::jsonb), \
                ($2, 'identity_confirmed', '{}'::jsonb)",
    )
    .bind(user_a)
    .bind(user_b)
    .execute(&pool)
    .await
    .expect("insert events for both users");

    let count: i64 = as_app_admin!(&pool, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM verification_events WHERE user_id IN ($1, $2)",
        )
        .bind(user_a)
        .bind(user_b)
        .fetch_one(&mut *conn)
        .await
        .unwrap()
    });

    assert_eq!(
        count, 2,
        "app_admin must see both users' verification_events via admin-bypass policy",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Coverage extension — high-risk tables (Commit B).
//
// soultokens, background_checks, presence_events, visit_attestations carry
// the most sensitive data on the platform. These tests prove the
// `*_self`/`*_admin` policy pair fires correctly on each, using the same
// `as_app_user!` / `as_app_admin!` helpers as the verification_events
// proofs above.
//
// Setup helpers below seed minimal valid rows for each table. The test
// of-record on `verification_events` covers the policy *pattern*; these
// extensions cover the *surface*.
// ─────────────────────────────────────────────────────────────────────────────

/// Seed a `business` soultoken row owned by `holder_user_id`. Returns the
/// soultoken row id. Uses `token_type='business'` to avoid the four
/// `user_soultoken_requires_*` CHECK constraints that gate `'user'` rows.
async fn seed_business_soultoken(pool: &PgPool, holder_user_id: i32, display_code: &str) -> i32 {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (owner_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'attested') RETURNING id",
    ).bind(SafeEmail().fake::<String>()).fetch_one(pool).await.unwrap();
    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('rls-st-loc', 'box_fraise_store', '1 ST', 'America/Edmonton') RETURNING id",
    ).fetch_one(pool).await.unwrap();
    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'rls-st-biz', 'active') RETURNING id",
    ).bind(loc_id).bind(owner_id).fetch_one(pool).await.unwrap();
    let (st_id,): (i32,) = sqlx::query_as(
        "INSERT INTO soultokens \
         (uuid, display_code, display_code_key_version, schema_version, \
          holder_user_id, token_type, business_id, issued_at, expires_at) \
         VALUES (gen_random_uuid(), $1, 1, 1, $2, 'business', $3, \
                 now(), now() + INTERVAL '1 year') RETURNING id",
    ).bind(display_code).bind(holder_user_id).bind(biz_id)
     .fetch_one(pool).await.unwrap();
    st_id
}

/// Seed a minimal `background_checks` row for `user_id`. Requires an
/// `identity_credentials` row (NOT NULL FK). Returns the check id.
async fn seed_background_check(pool: &PgPool, user_id: i32) -> i32 {
    let (cred_id,): (i32,) = sqlx::query_as(
        "INSERT INTO identity_credentials \
         (user_id, credential_type, external_session_id, verified_at, \
          cooling_ends_at, cooling_app_opens_required) \
         VALUES ($1, 'stripe_identity', $2, now(), now() + INTERVAL '7 days', 3) \
         RETURNING id",
    ).bind(user_id).bind(format!("rls-cred-{user_id}"))
     .fetch_one(pool).await.unwrap();

    let (check_id,): (i32,) = sqlx::query_as(
        "INSERT INTO background_checks \
         (user_id, identity_credential_id, provider, check_type, status) \
         VALUES ($1, $2, 'comply_advantage', 'sanctions', 'pending') RETURNING id",
    ).bind(user_id).bind(cred_id).fetch_one(pool).await.unwrap();
    check_id
}

/// Seed a `presence_events` row for `user_id`. Requires a business
/// (NOT NULL FK). `session_id` and `beacon_id` are nullable.
async fn seed_presence_event(pool: &PgPool, user_id: i32) -> i32 {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let (owner_id,): (i32,) = sqlx::query_as(
        "INSERT INTO users (email, email_verified, verification_status) \
         VALUES ($1, true, 'attested') RETURNING id",
    ).bind(SafeEmail().fake::<String>()).fetch_one(pool).await.unwrap();
    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('rls-pe-loc', 'box_fraise_store', '1 PE', 'America/Edmonton') RETURNING id",
    ).fetch_one(pool).await.unwrap();
    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'rls-pe-biz', 'active') RETURNING id",
    ).bind(loc_id).bind(owner_id).fetch_one(pool).await.unwrap();
    let (event_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_events \
         (user_id, business_id, event_type, calendar_date, occurred_at) \
         VALUES ($1, $2, 'beacon_dwell', CURRENT_DATE, now()) RETURNING id",
    ).bind(user_id).bind(biz_id).fetch_one(pool).await.unwrap();
    event_id
}

/// Seed a `visit_attestations` row for `target_user_id`. Requires the
/// full FK chain: location → business → presence_threshold + staff_visit
/// + 4 distinct user roles. Returns the attestation id and the four user
/// ids `(staff, r1, r2, outsider)` so tests can assert isolation against
/// the outsider.
async fn seed_visit_attestation(pool: &PgPool, target_user_id: i32) -> (i32, i32, i32, i32) {
    use fake::{Fake, faker::internet::en::SafeEmail};
    let mk_user = |verification: &'static str| async move {
        let (id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified, verification_status) \
             VALUES ($1, true, $2) RETURNING id",
        ).bind(SafeEmail().fake::<String>()).bind(verification)
         .fetch_one(pool).await.unwrap();
        id
    };
    let staff_id = mk_user("attested").await;
    let r1_id    = mk_user("attested").await;
    let r2_id    = mk_user("attested").await;
    let outsider = mk_user("registered").await;

    let (loc_id,): (i32,) = sqlx::query_as(
        "INSERT INTO locations (name, location_type, address, timezone) \
         VALUES ('rls-va-loc', 'box_fraise_store', '1 VA', 'America/Edmonton') RETURNING id",
    ).fetch_one(pool).await.unwrap();
    let (biz_id,): (i32,) = sqlx::query_as(
        "INSERT INTO businesses (location_id, primary_holder_id, name, verification_status) \
         VALUES ($1, $2, 'rls-va-biz', 'active') RETURNING id",
    ).bind(loc_id).bind(r1_id).fetch_one(pool).await.unwrap();
    let (threshold_id,): (i32,) = sqlx::query_as(
        "INSERT INTO presence_thresholds \
         (user_id, business_id, event_count, days_count, threshold_met_at) \
         VALUES ($1, $2, 3, 3, now()) RETURNING id",
    ).bind(target_user_id).bind(biz_id).fetch_one(pool).await.unwrap();
    let (visit_id,): (i32,) = sqlx::query_as(
        "INSERT INTO staff_visits \
         (location_id, staff_id, scheduled_at, window_hours, expected_box_count, \
          arrived_at, status, visit_type) \
         VALUES ($1, $2, now(), 4, 5, now(), 'in_progress', 'combined') RETURNING id",
    ).bind(loc_id).bind(staff_id).fetch_one(pool).await.unwrap();
    let (attest_id,): (i32,) = sqlx::query_as(
        "INSERT INTO visit_attestations \
         (visit_id, user_id, staff_id, presence_threshold_id, \
          assigned_reviewer_1_id, assigned_reviewer_2_id, status, \
          co_sign_deadline) \
         VALUES ($1, $2, $3, $4, $5, $6, 'co_sign_pending', \
                 now() + INTERVAL '48 hours') RETURNING id",
    ).bind(visit_id).bind(target_user_id).bind(staff_id)
     .bind(threshold_id).bind(r1_id).bind(r2_id)
     .fetch_one(pool).await.unwrap();

    (attest_id, staff_id, r1_id, outsider)
}

// ── soultokens ──────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../server/migrations")]
async fn user_cannot_see_another_users_soultokens(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, user_b) = create_two_users(&pool).await;
    seed_business_soultoken(&pool, user_b, "AAAA-BBBB-CCCC").await;

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM soultokens WHERE holder_user_id = $1",
        ).bind(user_b).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 0, "user_a must not see user_b's soultoken");
}

#[sqlx::test(migrations = "../server/migrations")]
async fn user_can_see_own_soultoken(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, _) = create_two_users(&pool).await;
    seed_business_soultoken(&pool, user_a, "DDDD-EEEE-FFFF").await;

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM soultokens WHERE holder_user_id = $1",
        ).bind(user_a).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 1, "user_a must see their own soultoken");
}

// ── background_checks ──────────────────────────────────────────────────────

#[sqlx::test(migrations = "../server/migrations")]
async fn user_cannot_see_another_users_background_checks(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, user_b) = create_two_users(&pool).await;
    seed_background_check(&pool, user_b).await;

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM background_checks WHERE user_id = $1",
        ).bind(user_b).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 0, "user_a must not see user_b's background_checks");
}

#[sqlx::test(migrations = "../server/migrations")]
async fn user_can_see_own_background_checks(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, _) = create_two_users(&pool).await;
    seed_background_check(&pool, user_a).await;

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM background_checks WHERE user_id = $1",
        ).bind(user_a).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 1, "user_a must see their own background_checks");
}

// ── presence_events ─────────────────────────────────────────────────────────

#[sqlx::test(migrations = "../server/migrations")]
async fn user_cannot_see_another_users_presence_events(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, user_b) = create_two_users(&pool).await;
    seed_presence_event(&pool, user_b).await;

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM presence_events WHERE user_id = $1",
        ).bind(user_b).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 0, "user_a must not see user_b's presence_events");
}

#[sqlx::test(migrations = "../server/migrations")]
async fn user_can_see_own_presence_events(pool: PgPool) {
    setup_grants(&pool).await;
    let (user_a, _) = create_two_users(&pool).await;
    seed_presence_event(&pool, user_a).await;

    let count: i64 = as_app_user!(&pool, user_a, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM presence_events WHERE user_id = $1",
        ).bind(user_a).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 1, "user_a must see their own presence_events");
}

// ── visit_attestations ──────────────────────────────────────────────────────
//
// `visit_attestations_self` policy permits SELECT to any of FOUR principals:
// `user_id`, `staff_id`, `assigned_reviewer_1_id`, `assigned_reviewer_2_id`.
// The "isolation" test must therefore use a fifth user who is none of those —
// `seed_visit_attestation` returns an `outsider` id specifically for this.
//
// The "own-attestation" test uses the attested user (`user_id` column) to
// demonstrate the `user_id =` branch of the policy fires.

#[sqlx::test(migrations = "../server/migrations")]
async fn user_cannot_see_another_users_attestations(pool: PgPool) {
    setup_grants(&pool).await;
    let (target_user, _) = create_two_users(&pool).await;
    let (attest_id, _staff, _r1, outsider) = seed_visit_attestation(&pool, target_user).await;

    let count: i64 = as_app_user!(&pool, outsider, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM visit_attestations WHERE id = $1",
        ).bind(attest_id).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 0,
        "outsider (not user/staff/either reviewer) must not see the attestation");
}

#[sqlx::test(migrations = "../server/migrations")]
async fn user_can_see_own_attestation(pool: PgPool) {
    setup_grants(&pool).await;
    let (target_user, _) = create_two_users(&pool).await;
    let (attest_id, _staff, _r1, _outsider) = seed_visit_attestation(&pool, target_user).await;

    let count: i64 = as_app_user!(&pool, target_user, |conn| {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM visit_attestations WHERE id = $1",
        ).bind(attest_id).fetch_one(&mut *conn).await.unwrap()
    });
    assert_eq!(count, 1, "attested user (user_id column) must see their own attestation");
}
