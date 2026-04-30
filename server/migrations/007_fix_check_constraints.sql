-- Migration 007: Fix missing values in CHECK constraints
--
-- Two CHECK constraints were missing valid status/source values that the
-- application code already produces at runtime. Both cause silent DB errors
-- on otherwise-correct operations.
--
-- loyalty_events.source: 'nfc_tap' was introduced by the NFC sticker feature
--   but omitted from the original constraint (which only listed 'qr_stamp',
--   'stripe_webhook', 'manual'). Every NFC redemption would fail.
--
-- venue_orders.status: 'completed' is set by complete_order_from_square_inner
--   when Square fires order.updated → COMPLETED. The original constraint only
--   covered the payment flow states. Every drink-collection event would fail.
--
-- The DO blocks find the existing constraint by what it constrains rather than
-- by name — auto-generated constraint names vary across environments.
-- All existing rows satisfy the new constraints (their values are a strict
-- subset of the expanded allowlist), so no backfill is required.

-- ── loyalty_events.source ─────────────────────────────────────────────────────

DO $$
DECLARE v_name text;
BEGIN
    SELECT con.conname INTO v_name
    FROM pg_constraint con
    JOIN pg_class rel ON rel.oid = con.conrelid
    WHERE rel.relname = 'loyalty_events'
      AND con.contype = 'c'
      AND pg_get_constraintdef(con.oid) LIKE '%source%';

    IF v_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE loyalty_events DROP CONSTRAINT %I', v_name);
    END IF;
END $$;

ALTER TABLE loyalty_events
    ADD CONSTRAINT loyalty_events_source_check
    CHECK (source IN ('qr_stamp', 'nfc_tap', 'stripe_webhook', 'manual'));

-- ── venue_orders.status ───────────────────────────────────────────────────────

DO $$
DECLARE v_name text;
BEGIN
    SELECT con.conname INTO v_name
    FROM pg_constraint con
    JOIN pg_class rel ON rel.oid = con.conrelid
    WHERE rel.relname = 'venue_orders'
      AND con.contype = 'c'
      AND pg_get_constraintdef(con.oid) LIKE '%status%';

    IF v_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE venue_orders DROP CONSTRAINT %I', v_name);
    END IF;
END $$;

ALTER TABLE venue_orders
    ADD CONSTRAINT venue_orders_status_check
    CHECK (status IN ('pending', 'paid', 'pushed_to_square', 'completed', 'failed'));
