-- =============================================================
-- Hardening cleanup #8 — per-endpoint rate-limit configuration
--
-- Background: today the rate limiter runs as middleware BEFORE auth
-- extraction (server/src/app.rs middleware stack), so it can only key
-- on IP. Per-user limits require either:
--   (a) splitting the limiter into a post-auth middleware (extracts
--       the JWT claims, then applies the per-user counter), or
--   (b) per-route extractors that do the rate check inline.
-- Either change is significant and out of scope for cleanup #8.
--
-- This migration ships the *limit values* into platform_configuration
-- so they're operator-tunable when the architectural change lands —
-- the values become the source of truth for the future limiter, and
-- can be tuned without redeploying. Reads should go through
-- `platform_configuration::service::get_configuration` (cached).
--
-- Idempotent: re-applying is a no-op once the rows exist.
-- =============================================================

INSERT INTO platform_configuration
    (key,                                       value, value_type, description,                                              cache_ttl_seconds)
VALUES
    ('rate_limit_attestations_per_hour',        '10',  'integer',  'Max attestation initiations per user per hour',           300),
    ('rate_limit_background_checks_per_day',    '5',   'integer',  'Max background-check initiations per user per day',       300),
    ('rate_limit_identity_initiations_per_day', '3',   'integer',  'Max identity verifications per user per day',             300),
    ('rate_limit_dorotka_per_hour',             '20',  'integer',  'Max Dorotka queries per user per hour',                   300)
ON CONFLICT (key) DO NOTHING;
