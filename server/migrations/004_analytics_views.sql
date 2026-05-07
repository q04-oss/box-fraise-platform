-- =============================================================
-- Hardening §5 — Analytics views
--
-- Pre-computed aggregate views that drive Grafana / Metabase dashboards
-- without requiring those tools to write protocol-aware SQL. The HTTP
-- analytics surface in `domain/analytics` queries the base tables directly
-- because it needs to compute drop-off rates from the funnel; these views
-- exist so an external BI tool connecting as `app_readonly` can render the
-- same data with a single SELECT.
--
-- Idempotent: every CREATE OR REPLACE / GRANT is safe to re-run.
--
-- Schema note: the section spec referred to a `users.banned_at` column;
-- the live schema's banned flag is `users.is_banned BOOLEAN` (see
-- migration 001). Using `NOT is_banned` accordingly.
-- =============================================================

CREATE OR REPLACE VIEW analytics_verification_funnel AS
SELECT
    verification_status                                             AS stage,
    COUNT(*)::bigint                                                AS user_count,
    AVG(EXTRACT(EPOCH FROM (now() - created_at)) / 86400.0)::float8 AS avg_days_at_stage
FROM users
WHERE deleted_at IS NULL AND NOT is_banned
GROUP BY verification_status;

CREATE OR REPLACE VIEW analytics_daily_activity AS
SELECT
    DATE(created_at)                                                              AS date,
    COUNT(*) FILTER (WHERE event_type = 'soultoken_issued')::bigint               AS soultokens_issued,
    COUNT(*) FILTER (WHERE event_type = 'attestation_approved')::bigint           AS attestations_approved,
    COUNT(*) FILTER (WHERE event_type = 'cooling_period_completed')::bigint       AS cooling_completions
FROM verification_events
WHERE created_at > now() - INTERVAL '90 days'
GROUP BY DATE(created_at)
ORDER BY date DESC;

CREATE OR REPLACE VIEW analytics_soultoken_health AS
SELECT
    COUNT(*)::bigint                                                       AS total_issued,
    COUNT(*) FILTER (WHERE revoked_at IS NULL AND expires_at >  now())::bigint AS active,
    COUNT(*) FILTER (WHERE revoked_at IS NOT NULL)::bigint                 AS revoked,
    COUNT(*) FILTER (WHERE revoked_at IS NULL AND expires_at <= now())::bigint AS expired
FROM soultokens
WHERE token_type = 'user';

-- Surface the views to the analytics-only role created in migration 002.
GRANT SELECT ON analytics_verification_funnel TO app_readonly;
GRANT SELECT ON analytics_daily_activity      TO app_readonly;
GRANT SELECT ON analytics_soultoken_health    TO app_readonly;
