-- =============================================================
-- Hardening §9 — Data compliance
--
-- Adds the consent_records audit table and documents data-retention
-- policies on existing tables via COMMENT ON. The retention pruning
-- daemon (server/src/tasks/retention.rs) enforces the JWT-revocation
-- and magic-link policies; everything else is policy-only and enforced
-- by service-layer code (e.g. `request_erasure` anonymises in place
-- rather than DELETE'ing).
-- =============================================================

-- ── Consent records ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS consent_records (
    id              SERIAL PRIMARY KEY,
    user_id         INTEGER NOT NULL REFERENCES users(id),
    consent_type    TEXT NOT NULL,
                    -- one of: platform_terms, privacy_policy, data_processing,
                    -- background_checks, presence_tracking
    consent_version TEXT NOT NULL DEFAULT '1.0',
    granted         BOOLEAN NOT NULL,
    ip_address      TEXT,
    user_agent      TEXT,
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_consent_records_user_id
    ON consent_records(user_id);

COMMENT ON TABLE consent_records IS
    'Hardening §9. Append-only consent log. Never delete rows. '
    'Revocation is recorded via the revoked_at timestamp on the '
    'original grant row OR by inserting a new row with granted=false. '
    'Retention: indefinite — required for legal evidence.';

-- ── Retention policy comments on existing tables ────────────────────────────
-- These are documentation-only. The retention pruning daemon enforces
-- the policies marked "pruned via tasks/retention.rs"; everything else
-- is enforced by service-layer code or operational procedure.

COMMENT ON TABLE users IS
    E'BFIP Section 13. Core user identity record. Soft-deleted via deleted_at. '
    'Filter active users with WHERE deleted_at IS NULL.\n'
    'Hardening §9 retention: anonymise on erasure (request_erasure service) — '
    'rows are never hard-deleted because they anchor immutable audit / '
    'verification / soultoken records. Erased rows have email rewritten to '
    'erased-{id}@deleted.boxfraise.com and personal identifiers nulled.';

COMMENT ON COLUMN soultokens.revoked_at IS
    'Set on revocation (BFIP §7.4) or surrender (BFIP §7.5). '
    'Hardening §9 retention: the row is retained 7 years after this timestamp '
    'for legal evidence — never hard-deleted.';

COMMENT ON COLUMN audit_events.created_at IS
    'Hardening §9 retention: 7 years. The bf_prevent_modification trigger '
    'already blocks UPDATE/DELETE on this table; pruning is therefore a '
    'manual operational task scheduled at the 7-year mark.';

COMMENT ON COLUMN background_checks.expires_at IS
    'Hardening §9 retention: rows retained 12 months after expires_at. '
    'Pruning is a manual operational task; the rows themselves are still '
    'INSERT-only at the application layer.';

COMMENT ON COLUMN verification_events.created_at IS
    'Hardening §9 retention: tied to user lifecycle. Anonymising the user '
    '(request_erasure) leaves these rows in place because they reference '
    'an integer user_id that no longer maps to a personal identity.';

COMMENT ON COLUMN jwt_revocations.expires_at IS
    'Hardening §9 retention: pruned via tasks/retention.rs after expires_at + '
    '24 hours. Token is unusable past expires_at, so the row exists only to '
    'block requests that arrive in that overlap window.';

COMMENT ON COLUMN magic_link_tokens.used_at IS
    'Hardening §9 retention: pruned via tasks/retention.rs after used_at + '
    '1 hour, OR after expires_at + 24 hours when never used. Single-use '
    'tokens have no further forensic value once consumed.';
