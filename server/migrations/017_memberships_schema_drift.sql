-- Migration 017: Formalise memberships schema drift.
--
-- Two columns were added to the production memberships table without migrations:
--
-- 1. renewal_notified (boolean) — the jobs layer checks and sets this flag to
--    avoid sending duplicate renewal reminder emails. The schema had
--    renewal_notified_at (timestamp) but the code uses a boolean.
--
-- 2. amount_cents DEFAULT 0 — early membership inserts did not always supply
--    amount_cents. A default of 0 is semantically correct for legacy rows
--    (free/founding memberships) and prevents NOT NULL violations from old paths.

ALTER TABLE memberships
    ADD COLUMN IF NOT EXISTS renewal_notified BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE memberships
    ALTER COLUMN amount_cents SET DEFAULT 0;
