-- =============================================================
-- Hardening Section 2c — production application user
--
-- Creates the non-superuser `app_user_prod` role that the production
-- application connects as via APP_USER_DATABASE_URL. Membership in the
-- `app_user` role (granted by migration 002) is what brings RLS policies
-- into effect — superusers bypass RLS via BYPASSRLS, this role does not.
--
-- Idempotent: re-applying is a no-op.
--
-- BEFORE PRODUCTION: rotate the password.
--   ALTER USER app_user_prod WITH PASSWORD '<long random secret>';
-- The placeholder below exists so the migration applies cleanly in dev.
-- It MUST be replaced before the role is exposed to a network-reachable DB.
-- =============================================================

DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user_prod') THEN
        CREATE USER app_user_prod WITH PASSWORD 'CHANGE_ME_BEFORE_PRODUCTION';
        GRANT app_user TO app_user_prod;
    END IF;
END $$;
