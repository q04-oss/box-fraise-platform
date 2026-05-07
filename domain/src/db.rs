use sqlx::{postgres::PgPoolOptions, PgConnection, PgPool};
use std::time::Duration;

// ── RLS context helpers (Hardening Section 2c) ───────────────────────────────
//
// These helpers set the per-transaction settings that RLS policies in
// `server/migrations/002_rls.sql` read via `current_setting('app.user_id', true)`
// and `current_setting('app.is_admin', true)`.
//
// IMPORTANT — invariants the caller MUST uphold:
//
// 1. Call these inside an explicit transaction (`pool.begin().await?`) and run
//    all RLS-protected queries on the SAME `&mut PgConnection` that the
//    transaction wraps. `set_config(..., is_local := true)` is transaction-
//    scoped; outside a transaction Postgres issues a warning and falls back
//    to session scope, which leaks across pooled connections and is the
//    failure mode that makes a "set on a freshly-acquired pool connection"
//    middleware unsafe.
//
// 2. `user_id` MUST come from validated JWT claims. Never accept a `user_id`
//    from a request body, query parameter, header, or any client-supplied
//    field — passing one of those here would let a caller bypass RLS by
//    impersonating any user.

/// Set the RLS user context for the current transaction.
///
/// Run this as the first statement after `BEGIN`, on the same `PgConnection`
/// the rest of the transaction's queries will use.
pub async fn set_rls_user_context(
    conn:    &mut PgConnection,
    user_id: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user_id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

/// Mark the current transaction as running with platform-admin authority.
///
/// Only call after the application layer has verified the requesting user
/// is `platform_admin` — RLS admin-bypass policies trust this flag without
/// re-checking the user's role.
pub async fn set_rls_admin_context(
    conn: &mut PgConnection,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT set_config('app.is_admin', 'true', true)")
        .execute(conn)
        .await?;
    Ok(())
}

// ── Connection pool ──────────────────────────────────────────────────────────

/// Pool sizing parameters (Hardening §6). The first three come from
/// `Config` (`DB_MAX_CONNECTIONS`, `DB_MIN_CONNECTIONS`,
/// `DB_ACQUIRE_TIMEOUT_SECS`); the idle/lifetime values stay hardcoded —
/// they are recycle hints for whatever pooler sits in front of Postgres
/// (PgBouncer, Supavisor) and aren't worth env-tuning.
pub struct PoolOptions {
    /// Upper bound on concurrent Postgres connections.
    pub max_connections:  u32,
    /// Connections kept warm so post-idle requests don't wait on TCP+TLS.
    pub min_connections:  u32,
    /// Fail-fast deadline when the pool is saturated.
    pub acquire_timeout:  Duration,
}

/// Create and return a Postgres connection pool.
///
/// `acquire_timeout` is "fail-fast under load" — saturated pool returns
/// `PoolTimedOut` rather than queuing indefinitely. Idle (10 min) and max
/// lifetime (30 min) ensure connections behind a pooler don't get stuck on
/// stale state.
pub async fn connect(url: &str, opts: PoolOptions) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(opts.max_connections)
        .min_connections(opts.min_connections)
        .acquire_timeout(opts.acquire_timeout)
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
        .connect(url)
        .await
        .map_err(Into::into)
}
