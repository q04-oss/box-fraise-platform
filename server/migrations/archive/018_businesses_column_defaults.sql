-- Migration 018: Add defaults to required businesses columns.
--
-- The production database allows inserting businesses with only a name,
-- but the schema has type/address/city/launched_at as NOT NULL without
-- defaults. Add sensible defaults so minimal test inserts work.

ALTER TABLE businesses
    ALTER COLUMN type        SET DEFAULT 'business',
    ALTER COLUMN address     SET DEFAULT '',
    ALTER COLUMN city        SET DEFAULT '',
    ALTER COLUMN launched_at SET DEFAULT NOW();
