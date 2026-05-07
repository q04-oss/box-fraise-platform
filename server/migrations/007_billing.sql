-- =============================================================
-- Hardening §10 — Billing scaffolding
--
-- The `business_subscriptions` table is forward-looking infrastructure
-- for the iOS-app subscription tiers. No Stripe webhook handling yet —
-- rows will be created/updated by future code paths that don't exist.
-- This migration only creates the schema and an admin read route is
-- wired up in the platform_configuration domain so existing rows can
-- be inspected once they show up.
--
-- Tier definitions:
--   standard      — free; basic presence verification
--   professional  — CA$49/month; priority staff visits, support capacity
--   trusted       — CA$149/month; dedicated visits, cleared verification, API access
-- =============================================================

CREATE TABLE IF NOT EXISTS business_subscriptions (
    id                      SERIAL PRIMARY KEY,
    business_id             INTEGER NOT NULL REFERENCES businesses(id),
    tier                    TEXT NOT NULL DEFAULT 'standard'
                            CHECK (tier IN ('standard', 'professional', 'trusted')),
    stripe_customer_id      TEXT,
    stripe_subscription_id  TEXT,
    current_period_start    TIMESTAMPTZ,
    current_period_end      TIMESTAMPTZ,
    cancelled_at            TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (business_id)
);

CREATE INDEX IF NOT EXISTS idx_business_subscriptions_business_id
    ON business_subscriptions(business_id);
CREATE INDEX IF NOT EXISTS idx_business_subscriptions_tier
    ON business_subscriptions(tier);

COMMENT ON TABLE business_subscriptions IS
    'Hardening §10. Per-business subscription tiers. UNIQUE(business_id) — '
    'one subscription per business; tier downgrades/upgrades happen via '
    'UPDATE, not row insertion. Stripe IDs are nullable for the standard '
    '(free) tier and for businesses provisioned before billing went live.';

GRANT SELECT ON business_subscriptions TO app_readonly;
