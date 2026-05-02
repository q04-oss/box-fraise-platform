/// Audit event writer — append-only log of security-relevant platform actions.
///
/// Every call inserts a row into `audit_events`. The table has a DB trigger
/// (`audit_events_immutable`) that rejects UPDATE and DELETE, so the audit
/// trail cannot be scrubbed after the fact.
///
/// Callers do not check the return value: audit failures are logged but never
/// surface as user-facing errors — a failed audit write must not block a
/// legitimate request.
///
/// # Column semantics
///
/// - `user_id`  — the user the event is *about* (the person who acted)
/// - `actor_id` — an admin acting *on behalf of* a user; `None` for self-service actions
/// - `metadata` — arbitrary JSON payload; include IP addresses here when relevant
///
/// # Event kinds (non-exhaustive)
///
///   user.registered              — new account created
///   user.login                   — successful authentication
///   auth.magic_link_invalid      — magic link token expired or already consumed
///   auth.login_blocked           — authentication rejected (banned account)
///   dorotka.ask                  — Dorotka AI query submitted
use serde_json::Value;
use sqlx::PgPool;

/// Write an audit event. Errors are logged and swallowed — audit failures
/// never propagate to the caller.
///
/// `user_id`  is the user who triggered the event (the subject).
/// `actor_id` is an admin acting on behalf of another user; pass `None` for
/// all self-service actions (the common case).
pub async fn write(
    pool:       &PgPool,
    user_id:    Option<i32>,
    actor_id:   Option<i32>,
    event_kind: &str,
    metadata:   Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_events (event_kind, user_id, actor_id, metadata)
         VALUES ($1, $2, $3, $4)"
    )
    .bind(event_kind)
    .bind(user_id)
    .bind(actor_id)
    .bind(&metadata)
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!(
            event_kind = event_kind,
            error      = %e,
            "audit write failed — event not recorded"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Confirms that a write actually lands in the database and that the
    /// columns contain exactly what was passed in.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn audit_write_inserts_row(pool: sqlx::PgPool) {
        // Insert a user so the FK on user_id resolves.
        let (user_id,): (i32,) = sqlx::query_as(
            "INSERT INTO users (email, email_verified) VALUES ('audit@test.com', true) RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        write(
            &pool,
            Some(user_id),
            None,
            "auth.login_blocked",
            serde_json::json!({ "reason": "banned", "via": "magic_link" }),
        )
        .await;

        let row: (String, Option<i32>, Option<i32>, serde_json::Value) = sqlx::query_as(
            "SELECT event_kind, user_id, actor_id, metadata FROM audit_events WHERE user_id = $1"
        )
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("audit row must exist after write()");

        assert_eq!(row.0, "auth.login_blocked");
        assert_eq!(row.1, Some(user_id));
        assert_eq!(row.2, None);
        assert_eq!(row.3["reason"], "banned");
        assert_eq!(row.3["via"], "magic_link");
    }

    /// Confirms that write() with no user_id (anonymous event) also lands correctly.
    #[sqlx::test(migrations = "../server/migrations")]
    async fn audit_write_anonymous_event_inserts_row(pool: sqlx::PgPool) {
        write(
            &pool,
            None,
            None,
            "auth.magic_link_invalid",
            serde_json::json!({ "reason": "token_expired_or_consumed" }),
        )
        .await;

        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_events WHERE event_kind = $1")
                .bind("auth.magic_link_invalid")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count, 1);
    }
}
