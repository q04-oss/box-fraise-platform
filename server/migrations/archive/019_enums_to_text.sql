-- Migration 019: Convert PostgreSQL enum columns to TEXT.
--
-- Custom enum types (chocolate, finish, order_status, membership_tier,
-- device_role) require explicit ::text casts in every query because sqlx's
-- prepared statement protocol sees the enum OID and refuses to decode it
-- as String without a cast. TEXT columns decode without any cast.
--
-- The Rust types already enforce valid values at the application boundary,
-- so the database-level constraint provided by enum types is redundant.
-- TEXT with application validation is simpler and more evolvable — adding
-- a new chocolate variety is a code change, not a migration.
--
-- USING clauses are required: PostgreSQL will not implicitly cast enum → text
-- in an ALTER COLUMN TYPE statement.

-- ── orders ────────────────────────────────────────────────────────────────────

ALTER TABLE orders
    ALTER COLUMN chocolate TYPE TEXT USING chocolate::text,
    ALTER COLUMN finish    TYPE TEXT USING finish::text,
    ALTER COLUMN status    TYPE TEXT USING status::text;

ALTER TABLE orders ALTER COLUMN status SET DEFAULT 'pending';

-- ── memberships ───────────────────────────────────────────────────────────────

ALTER TABLE memberships ALTER COLUMN tier TYPE TEXT USING tier::text;

-- ── devices ───────────────────────────────────────────────────────────────────

ALTER TABLE devices ALTER COLUMN role TYPE TEXT USING role::text;
ALTER TABLE devices ALTER COLUMN role SET DEFAULT 'user';

-- ── standing_orders (live table, also uses chocolate and finish enums) ─────────

ALTER TABLE standing_orders
    ALTER COLUMN chocolate TYPE TEXT USING chocolate::text,
    ALTER COLUMN finish    TYPE TEXT USING finish::text;

-- ── membership_waitlist (live table, uses membership_tier) ────────────────────

ALTER TABLE membership_waitlist ALTER COLUMN tier TYPE TEXT USING tier::text;

-- ── Drop the enum type definitions ───────────────────────────────────────────
-- These must be dropped after all columns that reference them are converted.

DROP TYPE IF EXISTS public.chocolate;
DROP TYPE IF EXISTS public.finish;
DROP TYPE IF EXISTS public.order_status;
DROP TYPE IF EXISTS public.membership_tier;
DROP TYPE IF EXISTS public.device_role;
