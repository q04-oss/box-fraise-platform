-- Migration 008: Add business_id to devices and orders
--
-- Motivation: removes JOIN workarounds for business scope enforcement.
--
-- Before this migration, device_collect had to join through employment_contracts
-- to verify a device belongs to the same business as an order. Similarly, order
-- queries had to join locations to derive the owning business. Both are now
-- first-class columns, enabling direct equality checks and indexed lookups.
--
-- Backfill strategy:
--   devices:  associate each device with its user's most recently started active
--             employment contract. Devices with no active contract get NULL.
--   orders:   derive business_id from the order's location. All existing orders
--             have a valid location_id so this backfill is complete.

-- ── devices ───────────────────────────────────────────────────────────────────

ALTER TABLE devices
    ADD COLUMN IF NOT EXISTS business_id INTEGER REFERENCES businesses(id) ON DELETE SET NULL;

UPDATE devices d
SET business_id = (
    SELECT ec.business_id
    FROM   employment_contracts ec
    WHERE  ec.user_id = d.user_id
      AND  ec.status  = 'active'
    ORDER  BY ec.created_at DESC
    LIMIT  1
);

CREATE INDEX IF NOT EXISTS idx_devices_business
    ON devices (business_id)
    WHERE business_id IS NOT NULL;

-- ── orders ────────────────────────────────────────────────────────────────────

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS business_id INTEGER REFERENCES businesses(id) ON DELETE SET NULL;

UPDATE orders o
SET business_id = l.business_id
FROM locations l
WHERE l.id = o.location_id;

CREATE INDEX IF NOT EXISTS idx_orders_business
    ON orders (business_id)
    WHERE business_id IS NOT NULL;
