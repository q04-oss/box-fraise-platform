-- =============================================================
-- Hardening §10 — Feature flags + admin ban metadata
--
-- Two changes bundled because Step 4 needs columns on `users` that the
-- ban tooling enforces, and splitting them out into a separate migration
-- would only add noise.
-- =============================================================

-- ── Feature flags ───────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS feature_flags (
    id                   SERIAL PRIMARY KEY,
    flag_name            TEXT NOT NULL UNIQUE,
    enabled              BOOLEAN NOT NULL DEFAULT false,
    description          TEXT NOT NULL,
    enabled_for_user_ids INTEGER[] NOT NULL DEFAULT '{}',
    enabled_for_pct      INTEGER NOT NULL DEFAULT 0
                         CHECK (enabled_for_pct BETWEEN 0 AND 100),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_feature_flags_name ON feature_flags(flag_name);

COMMENT ON TABLE feature_flags IS
    'Hardening §10. Runtime feature flags. Resolution order in '
    'platform_configuration::service::is_feature_enabled: enabled (global) → '
    'enabled_for_user_ids (allow-list) → enabled_for_pct (gradual rollout, '
    'stable per-user_id bucketing via user_id % 100).';

INSERT INTO feature_flags (flag_name, enabled, description) VALUES
    ('web3_payments',            false, 'Enable USDC on-chain payments via Optimism'),
    ('bfap_agent_credentials',   false, 'Enable BFAP agent credential issuance'),
    ('cleared_tier',             false, 'Enable cleared soultoken tier (full background checks)'),
    ('mesh_presence',            false, 'Enable BFMP mesh presence verification'),
    ('terminal_mode',            false, 'Enable CardputerZero terminal device mode')
ON CONFLICT (flag_name) DO NOTHING;

-- ── Admin ban metadata (Hardening §10 Step 4) ───────────────────────────────
ALTER TABLE users ADD COLUMN IF NOT EXISTS banned_at     TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS banned_reason TEXT;

COMMENT ON COLUMN users.banned_at IS
    'Hardening §10. When the platform_admin ban was applied. Cleared on unban. '
    'is_banned remains the source of truth for "is this user blocked"; '
    'banned_at + banned_reason exist for audit display only.';
