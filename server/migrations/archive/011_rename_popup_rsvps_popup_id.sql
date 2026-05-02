-- Migration 011: Rename popup_rsvps.popup_id → business_id
--
-- Active code in popups/routes.rs already uses 'business_id' as the column
-- name in all SQL (INSERT, SELECT, DELETE). The schema dump retained the old
-- name 'popup_id' from before the production rename, causing a drift between
-- the schema file and the live database.
--
-- The DO block makes this idempotent: if the column is already named
-- 'business_id' (as it is in production), this migration is a no-op.

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name   = 'popup_rsvps'
          AND column_name  = 'popup_id'
    ) THEN
        ALTER TABLE popup_rsvps RENAME COLUMN popup_id TO business_id;
    END IF;
END;
$$;
