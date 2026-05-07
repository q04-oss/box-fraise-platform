-- =============================================================
-- Hardening cleanup #1 — consolidate the platform_admin path
--
-- Background: the platform had two parallel platform_admin enforcement
-- surfaces:
--   1. users.is_platform_admin (boolean) — what every authorization
--      check in the codebase actually reads.
--   2. staff_roles row with role='platform_admin' — granted via
--      grant_staff_role with a two-person-rule branch, but never
--      queried by any service authorization check.
--
-- The dual surface created the impression of rigour without runtime
-- effect: revoking the staff_roles row left the user fully admin via
-- the boolean column. This migration deletes the dead surface.
--
-- Idempotent: re-applying is a no-op once the constraint is in place.
-- =============================================================

-- Drop any historical platform_admin rows. There shouldn't be any in dev
-- (no test calls grant_staff_role with this role), but production may
-- have rows from manual operator action.
DELETE FROM staff_roles WHERE role = 'platform_admin';

-- Replace the role CHECK with the narrower set. Operational roles only.
ALTER TABLE staff_roles
    DROP CONSTRAINT IF EXISTS staff_roles_role_check;

ALTER TABLE staff_roles
    ADD CONSTRAINT staff_roles_role_check
    CHECK (role IN ('delivery_staff', 'attestation_reviewer'));

COMMENT ON COLUMN staff_roles.role IS
    'Operational role only: delivery_staff or attestation_reviewer. '
    'Platform-admin status is controlled exclusively via '
    'users.is_platform_admin — see docs/ACCESS_CONTROL_MATRIX.md.';

-- Refresh the table-level comment that previously described the two-person
-- platform_admin rule. The two-person rule is gone; the no_self_confirmation
-- check still constrains the optional confirmed_by field for any future
-- operational use, but it is no longer required for any role.
COMMENT ON TABLE staff_roles IS
    'BFIP Section 6.1, 10. Box Fraise operational authority. '
    'Roles: delivery_staff (location-specific field ops, attestations, support); '
    'attestation_reviewer (remote co-signing only). '
    'Platform-admin authority is NOT a staff_role — it is the boolean '
    'users.is_platform_admin column (see docs/ACCESS_CONTROL_MATRIX.md §5). '
    'delivery_staff must have location_id (delivery_staff_requires_location). '
    'no_self_confirmation prevents self-confirmation of any optional confirmed_by. '
    'Active roles: WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()).';
