-- Migration 020: Drop the dead renewal_notified_at column from memberships.
--
-- The schema originally had renewal_notified_at (timestamp) but the jobs
-- layer uses renewal_notified (boolean, added in migration 017). Nothing
-- in the Rust codebase reads renewal_notified_at.

ALTER TABLE memberships DROP COLUMN IF EXISTS renewal_notified_at;
