-- Migration 004: Loyalty programme
--
-- business_loyalty_config: one row per business, defines the reward structure.
-- Populated by the business operator; defaults are intentionally absent so
-- each business explicitly configures their programme before it goes live.
--
-- loyalty_events: append-only ledger. The trigger below rejects any UPDATE
-- or DELETE — the balance is always derived from the full event history.
-- Adjustments are compensating events (event_type = 'steep_adjusted') with
-- a signed delta in metadata, never retroactive edits.
--
-- idempotency_key UNIQUE: the QR token UUID doubles as the idempotency key.
-- A re-delivered webhook or a retried stamp request hits the UNIQUE constraint
-- and is rejected cleanly — no double-stamps.

-- ── Loyalty configuration per business ───────────────────────────────────────

CREATE TABLE business_loyalty_config (
    business_id       INTEGER     NOT NULL PRIMARY KEY REFERENCES businesses(id) ON DELETE CASCADE,
    steeps_per_reward INTEGER     NOT NULL DEFAULT 10 CHECK (steeps_per_reward > 0),
    -- Shown to customers in the app: "1 more steep until your free matcha"
    reward_description TEXT       NOT NULL DEFAULT 'one free drink',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Loyalty events ────────────────────────────────────────────────────────────

CREATE TABLE loyalty_events (
    id               BIGSERIAL    PRIMARY KEY,
    user_id          INTEGER      NOT NULL REFERENCES users(id)      ON DELETE CASCADE,
    business_id      INTEGER      NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    -- 'steep_earned'    — one stamp recorded
    -- 'reward_redeemed' — customer claimed a reward
    -- 'steep_adjusted'  — compensating event; metadata contains { "delta": -1, "reason": "..." }
    event_type       TEXT         NOT NULL
                                  CHECK (event_type IN ('steep_earned', 'reward_redeemed', 'steep_adjusted')),
    -- 'qr_stamp'        — staff scanned customer QR
    -- 'stripe_webhook'  — auto-stamp on completed in-app payment
    -- 'manual'          — operator-initiated adjustment
    source           TEXT         NOT NULL
                                  CHECK (source IN ('qr_stamp', 'stripe_webhook', 'manual')),
    -- QR token UUID, Stripe payment intent ID, or operator note.
    -- UNIQUE enforces idempotency: re-delivered webhooks hit this constraint.
    idempotency_key  TEXT         NOT NULL UNIQUE,
    -- Additional context: { "delta": -2 } for adjustments,
    -- { "stripe_payment_intent_id": "pi_..." } for webhook events.
    metadata         JSONB        NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ  NOT NULL DEFAULT now()
);

-- Append-only enforcement: UPDATE and DELETE are forbidden.
CREATE OR REPLACE FUNCTION loyalty_events_immutable()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'loyalty_events rows are immutable — use compensating events';
END;
$$;

CREATE TRIGGER enforce_loyalty_immutability
    BEFORE UPDATE OR DELETE ON loyalty_events
    FOR EACH ROW EXECUTE FUNCTION loyalty_events_immutable();

-- Indexes for the two primary read patterns:
--   balance query: all events for a user at a business
--   history query: same, ordered by time
CREATE INDEX idx_loyalty_user_business ON loyalty_events (user_id, business_id, created_at DESC);
-- For the dashboard: all recent events at a business across all users
CREATE INDEX idx_loyalty_business ON loyalty_events (business_id, created_at DESC);
