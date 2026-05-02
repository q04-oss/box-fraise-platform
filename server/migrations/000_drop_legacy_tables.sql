-- Drop pre-BFIP tables that are superseded by the BFIP v0.1.2 schema.
-- Safe to run on a fresh database (IF EXISTS).

DROP TABLE IF EXISTS messages CASCADE;
DROP TABLE IF EXISTS user_keys CASCADE;
DROP TABLE IF EXISTS one_time_pre_keys CASCADE;
DROP TABLE IF EXISTS key_challenges CASCADE;
