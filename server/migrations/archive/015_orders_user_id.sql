-- Migration 015: Add user_id to orders table.
--
-- The orders table was created without user_id (it was later added directly
-- to production, creating schema drift). This migration formalises the column
-- so fresh test databases match the production schema.
--
-- user_id is nullable — App Clip guest orders and walk-in orders have no user.

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS user_id INTEGER REFERENCES users(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_orders_user
    ON orders (user_id)
    WHERE user_id IS NOT NULL;
