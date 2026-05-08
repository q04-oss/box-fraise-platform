//! Per-request RLS-scoped transaction wrappers (Hardening cleanup #3).
//!
//! Every authenticated request that touches RLS-protected tables must run
//! its queries inside a single transaction with the per-transaction setting
//! `app.user_id` (or `app.is_admin`) in place. The RLS policies in
//! `server/migrations/002_rls.sql` read those settings via
//! `current_setting('app.user_id', true)` to scope row visibility to the
//! caller.
//!
//! The wrappers in this module enforce the invariant by construction:
//!
//! 1. `begin()` opens the transaction and runs `set_config(..., true)`
//!    on the *same* connection in one atomic step. There is no way to
//!    obtain an `RlsTransaction` whose connection lacks the setting.
//!
//! 2. `as_mut()` exposes `&mut PgConnection` — the only Executor type
//!    that guarantees subsequent queries land on that same connection.
//!    Passing `&PgPool` instead would re-enter the pool and silently
//!    bypass RLS (Postgres downgrades `set_config` to session scope
//!    outside a tx and the next pooled-connection user inherits it —
//!    the precise failure mode `domain::db::set_rls_user_context`
//!    documents).
//!
//! 3. `commit()` is explicit. Dropping the wrapper without commit is a
//!    rollback (the standard sqlx Transaction guarantee). Handlers
//!    follow the pattern: begin → call service with `&mut tx` → on
//!    `Ok`, commit; on `Err`, the `?` operator drops the wrapper and
//!    Postgres rolls back.
//!
//! ## Test reality
//!
//! In test and dev DBs the application connects as the `fraise` superuser
//! which has `BYPASSRLS`. The wrappers still set `app.user_id` correctly
//! but the policies are not enforced. Real enforcement requires
//! `APP_USER_DATABASE_URL` to point at the non-superuser `app_user` role
//! (created by migration 003). The wrapper is therefore safe to roll out
//! ahead of the role switch — it adds correct plumbing now, and policy
//! enforcement turns on the day the env var changes.

use sqlx::{PgConnection, PgPool, Postgres, Transaction};

use crate::{
    db::{set_rls_admin_context, set_rls_user_context},
    error::{AppResult, DomainError},
};

/// A Postgres transaction that is guaranteed to have run
/// `set_config('app.user_id', <user_id>, true)` against its own connection.
///
/// All RLS-protected queries for the request must execute via `as_mut()`
/// against this transaction; running queries against a `&PgPool` instead
/// re-enters the pool and silently bypasses the per-tx setting.
pub struct RlsTransaction {
    inner:   Transaction<'static, Postgres>,
    /// Authenticated user id this transaction was opened for. Stored for
    /// logging and assertion only — RLS policies read the matching
    /// Postgres setting set in `begin`, not this field.
    user_id: i32,
}

impl RlsTransaction {
    /// Open a transaction and pin its `app.user_id` setting in one step.
    ///
    /// `user_id` MUST come from validated JWT claims. Passing a
    /// client-supplied id here would let the caller impersonate any user
    /// against the RLS policy set.
    pub async fn begin(pool: &PgPool, user_id: i32) -> AppResult<Self> {
        let mut tx = pool.begin().await.map_err(DomainError::Db)?;
        set_rls_user_context(&mut tx, user_id).await.map_err(DomainError::Db)?;
        Ok(Self { inner: tx, user_id })
    }

    /// The user id the RLS context is set for. For logging/assertions.
    pub fn user_id(&self) -> i32 {
        self.user_id
    }

    /// Borrow the underlying transaction's connection for query execution.
    /// Pass this to `sqlx::query*().fetch_*(tx.as_mut())` — never
    /// substitute `&pool` in a function that has been migrated to take a
    /// `RlsTransaction`.
    pub fn as_mut(&mut self) -> &mut PgConnection {
        &mut self.inner
    }

    /// Commit the transaction. Call on the success path of the handler
    /// after the service returns `Ok`. On the error path the wrapper is
    /// dropped instead and Postgres rolls back automatically.
    pub async fn commit(self) -> AppResult<()> {
        self.inner.commit().await.map_err(DomainError::Db)?;
        Ok(())
    }
}

/// A Postgres transaction tagged as running with platform-admin authority.
///
/// `set_rls_admin_context` sets `app.is_admin = 'true'`; the
/// admin-bypass policies in `002_rls.sql` trust this flag without
/// re-checking the user's `is_platform_admin`. Therefore: only call
/// `begin()` after the application layer has verified the requester is
/// `platform_admin` (typically via `users.is_platform_admin = true` —
/// the sole enforcement path consolidated in cleanup #1).
pub struct AdminRlsTransaction {
    inner: Transaction<'static, Postgres>,
}

impl AdminRlsTransaction {
    /// Open a transaction and tag it as admin-scope.
    pub async fn begin(pool: &PgPool) -> AppResult<Self> {
        let mut tx = pool.begin().await.map_err(DomainError::Db)?;
        set_rls_admin_context(&mut tx).await.map_err(DomainError::Db)?;
        Ok(Self { inner: tx })
    }

    /// Borrow the underlying transaction's connection for query execution.
    pub fn as_mut(&mut self) -> &mut PgConnection {
        &mut self.inner
    }

    /// Commit the transaction.
    pub async fn commit(self) -> AppResult<()> {
        self.inner.commit().await.map_err(DomainError::Db)?;
        Ok(())
    }
}
