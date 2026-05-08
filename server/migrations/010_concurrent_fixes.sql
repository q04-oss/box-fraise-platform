-- =============================================================
-- Concurrent-access fixes (Cleanup C / post-test findings)
--
-- Closes a race condition in `background_checks::service::initiate_check`
-- exposed by `concurrent_initiate_check_for_same_type` (server/tests/
-- integration.rs). The application-level dedup is a `SELECT pending →
-- INSERT` pattern; under READ COMMITTED, two concurrent transactions
-- both see "no pending" in their snapshots and both INSERT, leaving two
-- pending rows for the same `(user_id, check_type)`.
--
-- The partial UNIQUE INDEX makes the loser's INSERT fail with a
-- unique-violation that `initiate_check` maps to `DomainError::Conflict`.
--
-- Idempotent: `IF NOT EXISTS` guards re-application.
-- =============================================================

CREATE UNIQUE INDEX IF NOT EXISTS
    background_checks_one_pending_per_user_type
ON background_checks (user_id, check_type)
WHERE status = 'pending';
