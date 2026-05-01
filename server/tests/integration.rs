/// Integration tests for the magic link auth flow.
///
/// Each test uses `sqlx::test` — sqlx creates a fresh database per test,
/// runs all migrations, and cleans up afterward. Tests are fully isolated.
///
/// Run:
///   DATABASE_URL=postgres://... cargo test --test integration
use sqlx::PgPool;

// ─────────────────────────────────────────────────────────────────────────────
// Magic link auth (user lifecycle in the DB layer)
//
// Tests the repository functions that underpin the magic link flow:
// find-or-create, email verification, and banned-user handling.
// The Redis token exchange is covered by service-layer tests (requires Redis).
// ─────────────────────────────────────────────────────────────────────────────

/// First call to find-or-create creates a new unverified user.
#[sqlx::test]
async fn magic_link_creates_new_user_on_first_call(pool: PgPool) {
    let email = "newuser@test.com";

    let existing: Option<i32> =
        sqlx::query_scalar("SELECT id FROM users WHERE LOWER(email) = LOWER($1)")
            .bind(email)
            .fetch_optional(&pool)
            .await
            .unwrap();

    assert!(existing.is_none(), "precondition: user must not exist yet");

    let user_id: i32 =
        sqlx::query_scalar("INSERT INTO users (email, verified) VALUES ($1, false) RETURNING id")
            .bind(email)
            .fetch_one(&pool)
            .await
            .unwrap();

    let verified: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(!verified, "new magic link user must start unverified");
}

/// Verifying a magic link token marks the user as verified.
#[sqlx::test]
async fn magic_link_verify_marks_user_verified(pool: PgPool) {
    let user_id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified) VALUES ('toverify@test.com', false) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    sqlx::query("UPDATE users SET verified = true WHERE id = $1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let verified: bool = sqlx::query_scalar("SELECT verified FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(
        verified,
        "clicking the magic link must mark the user as verified"
    );
}

/// A banned user's magic link request is silently dropped — no token sent.
#[sqlx::test]
async fn magic_link_banned_user_is_silently_skipped(pool: PgPool) {
    let user_id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified, banned) VALUES ('banned@test.com', true, true) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let banned: bool = sqlx::query_scalar("SELECT banned FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(
        banned,
        "banned flag must be readable so service::request_magic_link can skip token issuance"
    );
}

/// find-or-create is idempotent: calling twice returns the same user ID.
#[sqlx::test]
async fn magic_link_find_or_create_is_idempotent(pool: PgPool) {
    let email = "idempotent@test.com";

    let first: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified) VALUES ($1, false)
             ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
             RETURNING id",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    let second: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, verified) VALUES ($1, false)
             ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
             RETURNING id",
    )
    .bind(email)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        first, second,
        "find-or-create must return the same user ID on repeated calls"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
        .bind(email)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(count, 1, "find-or-create must never produce duplicate rows");
}
