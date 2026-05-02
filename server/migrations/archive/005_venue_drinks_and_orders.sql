-- Migration 005: Venue drink menu + in-app orders
--
-- venue_drinks: the menu a business publishes on the platform.
-- Prices are stored here; the server always reads from this table when
-- building a PaymentIntent — the client never sends a price.
--
-- venue_orders: one row per customer order. Status transitions:
--   pending         → PaymentIntent created, not yet paid
--   paid            → payment_intent.succeeded webhook received
--   pushed_to_square→ Square POS order created successfully
--   failed          → Square push failed after payment (requires operator attention)
--
-- Idempotency: idempotency_key UNIQUE + stripe_payment_intent_id UNIQUE.
-- A re-delivered Stripe webhook hits the UNIQUE constraint and is ignored.
-- A retried POST /api/venue-orders with the same idempotency_key returns
-- the existing order's client_secret instead of creating a duplicate.
--
-- businesses.stripe_connect_account_id: the Stripe Express account ID for
-- each business. PaymentIntents use transfer_data[destination] = this ID
-- and application_fee_amount = platform cut. Null = Connect not set up.

-- Shared trigger function: keeps updated_at current on any UPDATE.
-- Created here because venue_orders is the first table that needs it;
-- safe to call CREATE OR REPLACE if a future migration adds another.
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

ALTER TABLE businesses
    ADD COLUMN IF NOT EXISTS stripe_connect_account_id TEXT;

-- ── Drink menu ────────────────────────────────────────────────────────────────

CREATE TABLE venue_drinks (
    id          BIGSERIAL    PRIMARY KEY,
    business_id INTEGER      NOT NULL REFERENCES businesses(id) ON DELETE CASCADE,
    name        TEXT         NOT NULL,
    description TEXT         NOT NULL DEFAULT '',
    price_cents INTEGER      NOT NULL CHECK (price_cents > 0),
    category    TEXT         NOT NULL DEFAULT 'drink',
    available   BOOLEAN      NOT NULL DEFAULT true,
    sort_order  INTEGER      NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_venue_drinks_business ON venue_drinks (business_id, sort_order)
    WHERE available = true;

-- ── Orders ────────────────────────────────────────────────────────────────────

CREATE TABLE venue_orders (
    id                        BIGSERIAL    PRIMARY KEY,
    user_id                   INTEGER      NOT NULL REFERENCES users(id),
    business_id               INTEGER      NOT NULL REFERENCES businesses(id),
    -- Set on PaymentIntent creation; UNIQUE enforces idempotency at DB level.
    stripe_payment_intent_id  TEXT         UNIQUE,
    -- Client-provided UUID; returned as-is if the same key is resubmitted.
    idempotency_key           TEXT         NOT NULL UNIQUE,
    -- Set after Square API responds; null until pushed_to_square.
    square_order_id           TEXT,
    status                    TEXT         NOT NULL DEFAULT 'pending'
                                           CHECK (status IN
                                               ('pending','paid','pushed_to_square','failed')),
    total_cents               INTEGER      NOT NULL CHECK (total_cents > 0),
    -- Fee retained by the platform. Recorded at creation time so it
    -- survives future PLATFORM_FEE_BIPS changes.
    platform_fee_cents        INTEGER      NOT NULL DEFAULT 0,
    notes                     TEXT         NOT NULL DEFAULT '',
    created_at                TIMESTAMPTZ  NOT NULL DEFAULT now(),
    updated_at                TIMESTAMPTZ  NOT NULL DEFAULT now()
);

CREATE INDEX idx_venue_orders_business ON venue_orders (business_id, created_at DESC);
CREATE INDEX idx_venue_orders_user     ON venue_orders (user_id, created_at DESC);
CREATE INDEX idx_venue_orders_status   ON venue_orders (status)
    WHERE status IN ('pending', 'paid');

CREATE TRIGGER venue_orders_updated_at
    BEFORE UPDATE ON venue_orders
    FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- ── Order line items ──────────────────────────────────────────────────────────

CREATE TABLE venue_order_items (
    id          BIGSERIAL PRIMARY KEY,
    order_id    BIGINT    NOT NULL REFERENCES venue_orders(id) ON DELETE CASCADE,
    drink_id    BIGINT    NOT NULL REFERENCES venue_drinks(id),
    -- Denormalised snapshot — menu changes never corrupt order history.
    drink_name  TEXT      NOT NULL,
    price_cents INTEGER   NOT NULL,
    quantity    INTEGER   NOT NULL DEFAULT 1 CHECK (quantity > 0)
);
