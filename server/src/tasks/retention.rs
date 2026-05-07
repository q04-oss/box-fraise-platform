//! Retention pruning daemon — Hardening §9.
//!
//! Runs once per day, removes rows that have outlived the retention policies
//! documented on `jwt_revocations.expires_at` and `magic_link_tokens.used_at`
//! in migration 005. Everything else (audit / verification / soultoken /
//! background-check tables) is policy-only and pruned manually at the
//! 7-year / 12-month marks.

use std::time::Duration;

use sqlx::PgPool;

const ONE_DAY_SECS: u64 = 86_400;

/// Sleep one day, then prune. The startup-delay shape (sleep first) is
/// intentional — we don't want a server restart loop to also be a churn
/// loop on the prunable tables.
pub async fn run_retention_pruning(pool: PgPool) {
    loop {
        tokio::time::sleep(Duration::from_secs(ONE_DAY_SECS)).await;
        prune_expired_jwt_revocations(&pool).await;
        prune_expired_magic_links(&pool).await;
        tracing::info!("Retention pruning completed");
    }
}

/// JWT revocation rows are useful only while a token they reference might
/// still be replayed. The token's own expiry is the cutoff; we keep the row
/// for an extra 24h to absorb clock drift between request and revocation
/// list, then prune.
pub async fn prune_expired_jwt_revocations(pool: &PgPool) {
    if let Err(e) = sqlx::query(
        "DELETE FROM jwt_revocations \
         WHERE expires_at < now() - INTERVAL '24 hours'"
    )
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "prune_expired_jwt_revocations failed");
    }
}

/// Magic links are single-use; once consumed they have no further forensic
/// value and can be dropped after a 1h grace window. Unused links past
/// expires_at + 24h are also dropped (the grace window covers the same
/// clock-drift concern as JWT revocations).
pub async fn prune_expired_magic_links(pool: &PgPool) {
    if let Err(e) = sqlx::query(
        "DELETE FROM magic_link_tokens \
         WHERE (used_at IS NOT NULL AND used_at < now() - INTERVAL '1 hour') \
            OR  expires_at < now() - INTERVAL '24 hours'"
    )
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "prune_expired_magic_links failed");
    }
}
