-- Migration 014: Document the is_dorotka platform admin flag.
--
-- The column name alone does not convey that this is a dangerous authorization
-- gate. This comment makes the risk visible to anyone inspecting the schema.

COMMENT ON COLUMN users.is_dorotka IS
    'Platform admin flag. True for the Dorotka service account only. '
    'Controls access to admin-level API endpoints. '
    'Do not set this to true for regular users.';
