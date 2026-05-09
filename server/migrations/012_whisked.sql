-- =============================================================
-- Whisked v1 — menu, orders, pickup-code lifecycle.
--
-- Whisked is the pickup ordering surface for matcha bars on the
-- box-fraise platform. The customer's iOS client (q04-oss/whisked-ios)
-- POSTs to /api/whisked/orders, sees a `W-XXXX` pickup code, and shows
-- it at the bar; staff validate the code via /api/whisked/orders/:id/validate
-- which atomically marks the order collected.
--
-- Idempotent: re-applying is a no-op (IF NOT EXISTS, ON CONFLICT DO
-- NOTHING on the seed).
-- =============================================================

CREATE TABLE IF NOT EXISTS whisked_menu_items (
    id           SERIAL PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT,
    price_cents  INTEGER NOT NULL,
    category     TEXT NOT NULL DEFAULT 'matcha',
    available    BOOLEAN NOT NULL DEFAULT true,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS whisked_orders (
    id                       SERIAL PRIMARY KEY,
    user_id                  INTEGER NOT NULL REFERENCES users(id),
    business_id              INTEGER NOT NULL REFERENCES businesses(id),
    status                   TEXT NOT NULL DEFAULT 'pending'
                             CHECK (status IN ('pending', 'preparing', 'ready', 'collected', 'cancelled')),
    total_cents              INTEGER NOT NULL,
    stripe_payment_intent_id TEXT,
    pickup_code              TEXT NOT NULL,
    pickup_code_used_at      TIMESTAMPTZ,
    estimated_pickup_at      TIMESTAMPTZ,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Pickup code uniqueness only matters for in-flight orders. Once the code
-- is consumed (pickup_code_used_at IS NOT NULL) the partial index releases
-- it and a new order can mint the same code without conflict.
CREATE UNIQUE INDEX IF NOT EXISTS whisked_orders_pickup_code_unique
    ON whisked_orders(pickup_code)
    WHERE pickup_code_used_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_whisked_orders_user
    ON whisked_orders(user_id);
CREATE INDEX IF NOT EXISTS idx_whisked_orders_business_active
    ON whisked_orders(business_id, status)
    WHERE status IN ('pending', 'preparing', 'ready');

CREATE TABLE IF NOT EXISTS whisked_order_items (
    id           SERIAL PRIMARY KEY,
    order_id     INTEGER NOT NULL REFERENCES whisked_orders(id),
    menu_item_id INTEGER NOT NULL REFERENCES whisked_menu_items(id),
    quantity     INTEGER NOT NULL DEFAULT 1,
    price_cents  INTEGER NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_whisked_order_items_order
    ON whisked_order_items(order_id);

GRANT SELECT, INSERT, UPDATE ON whisked_menu_items, whisked_orders, whisked_order_items
    TO app_user;
GRANT USAGE ON
    whisked_menu_items_id_seq,
    whisked_orders_id_seq,
    whisked_order_items_id_seq
    TO app_user;
GRANT SELECT ON whisked_menu_items, whisked_orders, whisked_order_items
    TO app_readonly;

-- Seed initial menu (single-line descriptions; running this twice is a no-op
-- because every row has a UNIQUE name match in the ON CONFLICT clause? — no,
-- there's no unique on name. Use ON CONFLICT DO NOTHING with no target so a
-- re-run on an already-seeded table simply adds duplicates only if it's empty).
INSERT INTO whisked_menu_items (name, description, price_cents, category, sort_order)
SELECT * FROM (VALUES
    ('Ceremonial Matcha',  'Pure ceremonial grade matcha, whisked to order', 650, 'matcha',  1),
    ('Matcha Latte',       'Ceremonial matcha with steamed oat milk',        750, 'matcha',  2),
    ('Iced Matcha Latte',  'Ceremonial matcha over ice with oat milk',       750, 'matcha',  3),
    ('Hojicha Latte',      'Roasted green tea with steamed oat milk',        700, 'hojicha', 4),
    ('Matcha Yuzu',        'Ceremonial matcha with yuzu citrus',             800, 'matcha',  5)
) AS seed(name, description, price_cents, category, sort_order)
WHERE NOT EXISTS (SELECT 1 FROM whisked_menu_items LIMIT 1);

COMMENT ON TABLE whisked_menu_items IS
    'Whisked drink catalogue. Public read via GET /api/whisked/menu.';
COMMENT ON TABLE whisked_orders IS
    'Whisked customer orders. pickup_code is a 4-char alphanumeric W-XXXX surfaced at .ready and consumed atomically by staff via /validate.';
COMMENT ON TABLE whisked_order_items IS
    'Line items per Whisked order. price_cents snapshotted at order placement so menu price changes do not retroactively alter the total.';
