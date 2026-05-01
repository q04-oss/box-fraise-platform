-- Migration 009: PI-anchor tables for Stripe webhook handlers
--
-- complete_order and complete_rsvp already anchor by stripe_payment_intent_id.
-- This migration provides the schema for the remaining handlers so they
-- can anchor to a DB row rather than trusting Stripe metadata.
--
-- Also creates earnings_ledger, which is referenced by the tip and portrait
-- payment domains but was absent from 000_initial_schema.sql.

-- ── 1. earnings_ledger ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS earnings_ledger (
    id                       SERIAL PRIMARY KEY,
    user_id                  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    amount_cents             BIGINT NOT NULL,
    type                     TEXT NOT NULL,
    stripe_payment_intent_id TEXT,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS earnings_ledger_user_id_idx
    ON earnings_ledger (user_id);

-- ── 2. tip_payments — anchor for complete_tip ─────────────────────────────────

CREATE TABLE IF NOT EXISTS tip_payments (
    id                       SERIAL PRIMARY KEY,
    business_id              INTEGER NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    amount_cents             BIGINT NOT NULL,
    stripe_payment_intent_id TEXT NOT NULL UNIQUE,
    status                   TEXT NOT NULL DEFAULT 'pending',
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── 3. portrait_purchase_intents — anchor for complete_portrait_purchase ──────

CREATE TABLE IF NOT EXISTS portrait_purchase_intents (
    id                       SERIAL PRIMARY KEY,
    token_id                 INTEGER NOT NULL REFERENCES portrait_tokens(id) ON DELETE CASCADE,
    buyer_user_id            INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    seller_user_id           INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    creator_user_id          INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    amount_cents             BIGINT NOT NULL,
    stripe_payment_intent_id TEXT NOT NULL UNIQUE,
    status                   TEXT NOT NULL DEFAULT 'pending',
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── 4. portal_access unique constraint ───────────────────────────────────────
-- portal/routes.rs uses ON CONFLICT (buyer_id, owner_id) but the initial schema
-- had no unique constraint on those columns. Add it so the clause is valid.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema    = 'public'
          AND table_name      = 'portal_access'
          AND constraint_name = 'portal_access_buyer_owner_unique'
    ) THEN
        ALTER TABLE portal_access ADD CONSTRAINT portal_access_buyer_owner_unique UNIQUE (buyer_id, owner_id);
    END IF;
END;
$$;

-- ── 5. identity_verification_sessions — anchor for handle_identity_verified ───
-- Session IDs (vs_xxx) are stored at creation time; the webhook resolves
-- user_id from the session ID rather than trusting metadata.

CREATE TABLE IF NOT EXISTS identity_verification_sessions (
    id                SERIAL PRIMARY KEY,
    user_id           INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    stripe_session_id TEXT NOT NULL UNIQUE,
    status            TEXT NOT NULL DEFAULT 'pending',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
