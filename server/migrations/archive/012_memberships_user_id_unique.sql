-- Migration 012: Add UNIQUE constraint on memberships.user_id
--
-- The memberships creation code uses ON CONFLICT (user_id) but the initial
-- schema had no UNIQUE constraint on that column, so the ON CONFLICT clause
-- would throw "there is no unique or exclusion constraint matching the
-- ON CONFLICT specification" at runtime.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints
        WHERE table_schema    = 'public'
          AND table_name      = 'memberships'
          AND constraint_name = 'memberships_user_id_key'
    ) THEN
        ALTER TABLE memberships ADD CONSTRAINT memberships_user_id_key UNIQUE (user_id);
    END IF;
END;
$$;
