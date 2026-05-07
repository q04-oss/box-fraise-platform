-- =============================================================
-- Hardening Section 2b — Row Level Security
-- Implements docs/ACCESS_CONTROL_MATRIX.md against all 38 tables.
--
-- Idempotent: every block uses IF NOT EXISTS / DROP POLICY IF EXISTS,
-- so re-applying this migration is a no-op.
--
-- Roles created cluster-wide:
--   app_user      — application runtime
--   app_readonly  — analytics / Grafana / Metabase
--   app_admin     — privileged platform operations
--
-- These policies are inert until the application connection role is
-- switched away from a superuser. The local `fraise` user has BYPASSRLS
-- by virtue of being the Docker postgres superuser, so cargo test and
-- the dev server bypass everything below.
--
-- Required runtime context for a non-superuser app_user connection:
--   SET LOCAL app.user_id = '<authenticated user_id>';
-- (Wiring this into the request lifecycle is a separate hardening pass.)
-- =============================================================

-- ── 1. ROLES ─────────────────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_user') THEN
        CREATE ROLE app_user;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_readonly') THEN
        CREATE ROLE app_readonly;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'app_admin') THEN
        CREATE ROLE app_admin;
    END IF;
END $$;

-- ── 2. BASE GRANTS ──────────────────────────────────────────
GRANT USAGE ON SCHEMA public TO app_user, app_readonly, app_admin;

-- app_user: SELECT/INSERT/UPDATE on every table; sequences for SERIALs.
GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA public TO app_user;
GRANT SELECT, USAGE         ON ALL SEQUENCES IN SCHEMA public TO app_user;

-- Append-only tables: only SELECT + INSERT for app_user. UPDATE/DELETE
-- additionally blocked by bf_prevent_modification trigger (defence in depth).
REVOKE UPDATE, DELETE ON
    audit_events,
    verification_events,
    attestation_attempts,
    gift_box_history,
    business_assessment_history,
    platform_configuration_history,
    audit_request_log
FROM app_user;

-- Soft-delete model — no DELETE anywhere for app_user.
REVOKE DELETE ON ALL TABLES IN SCHEMA public FROM app_user;

-- Column-level revokes for secret-bearing columns (matrix Section 4).
REVOKE SELECT (secret_key, previous_secret_key) ON beacons              FROM app_user;
REVOKE SELECT (token_hash)                      ON magic_link_tokens    FROM app_user;
REVOKE SELECT (identity_token_hash)             ON apple_auth_sessions  FROM app_user;

-- app_admin: same surface as app_user plus restricted columns and
-- admin-only tables. Still no DELETE — immutability is non-negotiable.
GRANT SELECT, INSERT, UPDATE ON ALL TABLES IN SCHEMA public TO app_admin;
GRANT SELECT, USAGE         ON ALL SEQUENCES IN SCHEMA public TO app_admin;
REVOKE UPDATE, DELETE ON
    audit_events,
    verification_events,
    attestation_attempts,
    gift_box_history,
    business_assessment_history,
    platform_configuration_history,
    audit_request_log
FROM app_admin;
REVOKE DELETE ON ALL TABLES IN SCHEMA public FROM app_admin;

-- app_readonly: SELECT only, with table- and column-level revokes for PII
-- and credential material.
GRANT SELECT ON ALL TABLES IN SCHEMA public TO app_readonly;
REVOKE SELECT ON
    magic_link_tokens,
    apple_auth_sessions,
    jwt_revocations,
    attestation_tokens,
    third_party_verification_attempts,
    audit_events,
    audit_request_log
FROM app_readonly;
REVOKE SELECT (secret_key, previous_secret_key)                     ON beacons               FROM app_readonly;
REVOKE SELECT (push_token, email, apple_id)                         ON users                 FROM app_readonly;
REVOKE SELECT (uuid)                                                ON soultokens            FROM app_readonly;
REVOKE SELECT (external_session_id, stripe_identity_report_id, response_hash)
                                                                    ON identity_credentials  FROM app_readonly;
REVOKE SELECT (external_check_id, response_hash)                    ON background_checks     FROM app_readonly;

-- ── 3. RLS POLICIES ─────────────────────────────────────────
-- Convention:
--   *_self      — owner-isolation policy on app_user
--   *_admin     — admin-bypass policy on app_admin
--   *_insert    — append-only INSERT-with-check on app_user
--   *_select    — append-only SELECT-by-owner on app_user
--
-- current_setting('app.user_id', true) returns NULL when unset; the cast
-- to integer leaves NULL, and `column = NULL` is NULL → policy denies.

-- =============================================================
-- 3a. RLS DISABLED (matrix Section 5)
-- magic_link_tokens, jwt_revocations, platform_configuration, locations
-- These tables intentionally have NO RLS. Access is controlled at the
-- role-grant level only.
-- =============================================================

-- =============================================================
-- 3b. STANDARD USER-ISOLATION TABLES
-- =============================================================

-- users (key column is `id`, not `user_id`)
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE users FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS users_self  ON users;
CREATE POLICY users_self  ON users FOR ALL TO app_user
    USING      (id = current_setting('app.user_id', true)::integer)
    WITH CHECK (id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS users_admin ON users;
CREATE POLICY users_admin ON users FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- apple_auth_sessions
ALTER TABLE apple_auth_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE apple_auth_sessions FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS apple_auth_sessions_self  ON apple_auth_sessions;
CREATE POLICY apple_auth_sessions_self  ON apple_auth_sessions FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS apple_auth_sessions_admin ON apple_auth_sessions;
CREATE POLICY apple_auth_sessions_admin ON apple_auth_sessions FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- identity_credentials
ALTER TABLE identity_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE identity_credentials FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS identity_credentials_self  ON identity_credentials;
CREATE POLICY identity_credentials_self  ON identity_credentials FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS identity_credentials_admin ON identity_credentials;
CREATE POLICY identity_credentials_admin ON identity_credentials FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- background_checks
ALTER TABLE background_checks ENABLE ROW LEVEL SECURITY;
ALTER TABLE background_checks FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS background_checks_self  ON background_checks;
CREATE POLICY background_checks_self  ON background_checks FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS background_checks_admin ON background_checks;
CREATE POLICY background_checks_admin ON background_checks FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- cooling_period_events
ALTER TABLE cooling_period_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE cooling_period_events FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS cooling_period_events_self  ON cooling_period_events;
CREATE POLICY cooling_period_events_self  ON cooling_period_events FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS cooling_period_events_admin ON cooling_period_events;
CREATE POLICY cooling_period_events_admin ON cooling_period_events FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- soultokens (key column is `holder_user_id`)
ALTER TABLE soultokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE soultokens FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS soultokens_self  ON soultokens;
CREATE POLICY soultokens_self  ON soultokens FOR ALL TO app_user
    USING      (holder_user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (holder_user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS soultokens_admin ON soultokens;
CREATE POLICY soultokens_admin ON soultokens FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- soultoken_renewals
ALTER TABLE soultoken_renewals ENABLE ROW LEVEL SECURITY;
ALTER TABLE soultoken_renewals FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS soultoken_renewals_self  ON soultoken_renewals;
CREATE POLICY soultoken_renewals_self  ON soultoken_renewals FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS soultoken_renewals_admin ON soultoken_renewals;
CREATE POLICY soultoken_renewals_admin ON soultoken_renewals FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- presence_sessions
ALTER TABLE presence_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE presence_sessions FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS presence_sessions_self  ON presence_sessions;
CREATE POLICY presence_sessions_self  ON presence_sessions FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS presence_sessions_admin ON presence_sessions;
CREATE POLICY presence_sessions_admin ON presence_sessions FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- presence_events
ALTER TABLE presence_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE presence_events FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS presence_events_self  ON presence_events;
CREATE POLICY presence_events_self  ON presence_events FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS presence_events_admin ON presence_events;
CREATE POLICY presence_events_admin ON presence_events FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- presence_thresholds
ALTER TABLE presence_thresholds ENABLE ROW LEVEL SECURITY;
ALTER TABLE presence_thresholds FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS presence_thresholds_self  ON presence_thresholds;
CREATE POLICY presence_thresholds_self  ON presence_thresholds FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS presence_thresholds_admin ON presence_thresholds;
CREATE POLICY presence_thresholds_admin ON presence_thresholds FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- attestation_tokens
ALTER TABLE attestation_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE attestation_tokens FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS attestation_tokens_self  ON attestation_tokens;
CREATE POLICY attestation_tokens_self  ON attestation_tokens FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS attestation_tokens_admin ON attestation_tokens;
CREATE POLICY attestation_tokens_admin ON attestation_tokens FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- support_bookings
ALTER TABLE support_bookings ENABLE ROW LEVEL SECURITY;
ALTER TABLE support_bookings FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS support_bookings_self  ON support_bookings;
CREATE POLICY support_bookings_self  ON support_bookings FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS support_bookings_admin ON support_bookings;
CREATE POLICY support_bookings_admin ON support_bookings FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- orders
ALTER TABLE orders ENABLE ROW LEVEL SECURITY;
ALTER TABLE orders FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS orders_self  ON orders;
CREATE POLICY orders_self  ON orders FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS orders_admin ON orders;
CREATE POLICY orders_admin ON orders FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- =============================================================
-- 3c. JOIN-BASED ISOLATION (qualifying_presence_events)
-- =============================================================

ALTER TABLE qualifying_presence_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE qualifying_presence_events FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS qualifying_presence_events_self  ON qualifying_presence_events;
CREATE POLICY qualifying_presence_events_self  ON qualifying_presence_events FOR ALL TO app_user
    USING (threshold_id IN (
        SELECT id FROM presence_thresholds
        WHERE user_id = current_setting('app.user_id', true)::integer
    ))
    WITH CHECK (threshold_id IN (
        SELECT id FROM presence_thresholds
        WHERE user_id = current_setting('app.user_id', true)::integer
    ));
DROP POLICY IF EXISTS qualifying_presence_events_admin ON qualifying_presence_events;
CREATE POLICY qualifying_presence_events_admin ON qualifying_presence_events FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- third_party_verification_attempts (admin-or-soultoken-holder)
ALTER TABLE third_party_verification_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE third_party_verification_attempts FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS third_party_verification_attempts_self  ON third_party_verification_attempts;
CREATE POLICY third_party_verification_attempts_self  ON third_party_verification_attempts FOR ALL TO app_user
    USING (requesting_business_soultoken_id IN (
        SELECT id FROM soultokens
        WHERE holder_user_id = current_setting('app.user_id', true)::integer
    ))
    WITH CHECK (requesting_business_soultoken_id IN (
        SELECT id FROM soultokens
        WHERE holder_user_id = current_setting('app.user_id', true)::integer
    ));
DROP POLICY IF EXISTS third_party_verification_attempts_admin ON third_party_verification_attempts;
CREATE POLICY third_party_verification_attempts_admin ON third_party_verification_attempts FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- =============================================================
-- 3d. BUSINESS-OWNERSHIP TABLES (join via businesses.primary_holder_id)
-- =============================================================

-- businesses
ALTER TABLE businesses ENABLE ROW LEVEL SECURITY;
ALTER TABLE businesses FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS businesses_self  ON businesses;
CREATE POLICY businesses_self  ON businesses FOR ALL TO app_user
    USING      (primary_holder_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (primary_holder_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS businesses_admin ON businesses;
CREATE POLICY businesses_admin ON businesses FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- beacons (key: business_id → primary_holder_id)
ALTER TABLE beacons ENABLE ROW LEVEL SECURITY;
ALTER TABLE beacons FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS beacons_self  ON beacons;
CREATE POLICY beacons_self  ON beacons FOR ALL TO app_user
    USING (business_id IN (
        SELECT id FROM businesses
        WHERE primary_holder_id = current_setting('app.user_id', true)::integer
    ))
    WITH CHECK (business_id IN (
        SELECT id FROM businesses
        WHERE primary_holder_id = current_setting('app.user_id', true)::integer
    ));
DROP POLICY IF EXISTS beacons_admin ON beacons;
CREATE POLICY beacons_admin ON beacons FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- beacon_rotation_log
ALTER TABLE beacon_rotation_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE beacon_rotation_log FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS beacon_rotation_log_self  ON beacon_rotation_log;
CREATE POLICY beacon_rotation_log_self  ON beacon_rotation_log FOR ALL TO app_user
    USING (beacon_id IN (
        SELECT b.id FROM beacons b
        JOIN businesses bz ON b.business_id = bz.id
        WHERE bz.primary_holder_id = current_setting('app.user_id', true)::integer
    ))
    WITH CHECK (beacon_id IN (
        SELECT b.id FROM beacons b
        JOIN businesses bz ON b.business_id = bz.id
        WHERE bz.primary_holder_id = current_setting('app.user_id', true)::integer
    ));
DROP POLICY IF EXISTS beacon_rotation_log_admin ON beacon_rotation_log;
CREATE POLICY beacon_rotation_log_admin ON beacon_rotation_log FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- beacon_health_log
ALTER TABLE beacon_health_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE beacon_health_log FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS beacon_health_log_self  ON beacon_health_log;
CREATE POLICY beacon_health_log_self  ON beacon_health_log FOR ALL TO app_user
    USING (beacon_id IN (
        SELECT b.id FROM beacons b
        JOIN businesses bz ON b.business_id = bz.id
        WHERE bz.primary_holder_id = current_setting('app.user_id', true)::integer
    ))
    WITH CHECK (beacon_id IN (
        SELECT b.id FROM beacons b
        JOIN businesses bz ON b.business_id = bz.id
        WHERE bz.primary_holder_id = current_setting('app.user_id', true)::integer
    ));
DROP POLICY IF EXISTS beacon_health_log_admin ON beacon_health_log;
CREATE POLICY beacon_health_log_admin ON beacon_health_log FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- quality_assessments (assessor OR business owner)
ALTER TABLE quality_assessments ENABLE ROW LEVEL SECURITY;
ALTER TABLE quality_assessments FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS quality_assessments_self  ON quality_assessments;
CREATE POLICY quality_assessments_self  ON quality_assessments FOR ALL TO app_user
    USING (
        assessor_id = current_setting('app.user_id', true)::integer
        OR business_id IN (
            SELECT id FROM businesses
            WHERE primary_holder_id = current_setting('app.user_id', true)::integer
        )
    )
    WITH CHECK (
        assessor_id = current_setting('app.user_id', true)::integer
        OR business_id IN (
            SELECT id FROM businesses
            WHERE primary_holder_id = current_setting('app.user_id', true)::integer
        )
    );
DROP POLICY IF EXISTS quality_assessments_admin ON quality_assessments;
CREATE POLICY quality_assessments_admin ON quality_assessments FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- =============================================================
-- 3e. STAFF TABLES
-- =============================================================

-- staff_roles
ALTER TABLE staff_roles ENABLE ROW LEVEL SECURITY;
ALTER TABLE staff_roles FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS staff_roles_self  ON staff_roles;
CREATE POLICY staff_roles_self  ON staff_roles FOR ALL TO app_user
    USING      (user_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS staff_roles_admin ON staff_roles;
CREATE POLICY staff_roles_admin ON staff_roles FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- reviewer_assignment_log
ALTER TABLE reviewer_assignment_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE reviewer_assignment_log FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS reviewer_assignment_log_self  ON reviewer_assignment_log;
CREATE POLICY reviewer_assignment_log_self  ON reviewer_assignment_log FOR ALL TO app_user
    USING      (reviewer_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (reviewer_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS reviewer_assignment_log_admin ON reviewer_assignment_log;
CREATE POLICY reviewer_assignment_log_admin ON reviewer_assignment_log FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- staff_visits (assigned staff OR business owner of location)
ALTER TABLE staff_visits ENABLE ROW LEVEL SECURITY;
ALTER TABLE staff_visits FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS staff_visits_self  ON staff_visits;
CREATE POLICY staff_visits_self  ON staff_visits FOR ALL TO app_user
    USING (
        staff_id = current_setting('app.user_id', true)::integer
        OR location_id IN (
            SELECT location_id FROM businesses
            WHERE primary_holder_id = current_setting('app.user_id', true)::integer
        )
    )
    WITH CHECK (
        staff_id = current_setting('app.user_id', true)::integer
        OR location_id IN (
            SELECT location_id FROM businesses
            WHERE primary_holder_id = current_setting('app.user_id', true)::integer
        )
    );
DROP POLICY IF EXISTS staff_visits_admin ON staff_visits;
CREATE POLICY staff_visits_admin ON staff_visits FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- staff_visit_notifications
ALTER TABLE staff_visit_notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE staff_visit_notifications FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS staff_visit_notifications_self  ON staff_visit_notifications;
CREATE POLICY staff_visit_notifications_self  ON staff_visit_notifications FOR ALL TO app_user
    USING      (recipient_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (recipient_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS staff_visit_notifications_admin ON staff_visit_notifications;
CREATE POLICY staff_visit_notifications_admin ON staff_visit_notifications FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- visit_signatures
ALTER TABLE visit_signatures ENABLE ROW LEVEL SECURITY;
ALTER TABLE visit_signatures FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS visit_signatures_self  ON visit_signatures;
CREATE POLICY visit_signatures_self  ON visit_signatures FOR ALL TO app_user
    USING      (reviewer_id = current_setting('app.user_id', true)::integer)
    WITH CHECK (reviewer_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS visit_signatures_admin ON visit_signatures;
CREATE POLICY visit_signatures_admin ON visit_signatures FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- visit_boxes (tapped user OR delivering staff via visit)
ALTER TABLE visit_boxes ENABLE ROW LEVEL SECURITY;
ALTER TABLE visit_boxes FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS visit_boxes_self  ON visit_boxes;
CREATE POLICY visit_boxes_self  ON visit_boxes FOR ALL TO app_user
    USING (
        tapped_by_user_id = current_setting('app.user_id', true)::integer
        OR visit_id IN (
            SELECT id FROM staff_visits
            WHERE staff_id = current_setting('app.user_id', true)::integer
        )
    )
    WITH CHECK (
        tapped_by_user_id = current_setting('app.user_id', true)::integer
        OR visit_id IN (
            SELECT id FROM staff_visits
            WHERE staff_id = current_setting('app.user_id', true)::integer
        )
    );
DROP POLICY IF EXISTS visit_boxes_admin ON visit_boxes;
CREATE POLICY visit_boxes_admin ON visit_boxes FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- =============================================================
-- 3f. ATTESTATION TABLES (multi-key — user OR staff OR reviewers)
-- =============================================================

-- visit_attestations
ALTER TABLE visit_attestations ENABLE ROW LEVEL SECURITY;
ALTER TABLE visit_attestations FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS visit_attestations_self  ON visit_attestations;
CREATE POLICY visit_attestations_self  ON visit_attestations FOR ALL TO app_user
    USING (
        user_id                = current_setting('app.user_id', true)::integer
        OR staff_id            = current_setting('app.user_id', true)::integer
        OR assigned_reviewer_1_id = current_setting('app.user_id', true)::integer
        OR assigned_reviewer_2_id = current_setting('app.user_id', true)::integer
    )
    WITH CHECK (
        user_id                = current_setting('app.user_id', true)::integer
        OR staff_id            = current_setting('app.user_id', true)::integer
        OR assigned_reviewer_1_id = current_setting('app.user_id', true)::integer
        OR assigned_reviewer_2_id = current_setting('app.user_id', true)::integer
    );
DROP POLICY IF EXISTS visit_attestations_admin ON visit_attestations;
CREATE POLICY visit_attestations_admin ON visit_attestations FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- =============================================================
-- 3g. APPEND-ONLY TABLES (separate INSERT and SELECT policies;
-- no UPDATE/DELETE policies because the trigger blocks them and
-- the role grant has UPDATE/DELETE revoked)
-- =============================================================

-- audit_events (admin-only SELECT; system INSERT for app_user)
ALTER TABLE audit_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_events FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS audit_events_insert ON audit_events;
CREATE POLICY audit_events_insert ON audit_events FOR INSERT TO app_user
    WITH CHECK (true);
-- Deliberately no SELECT policy for app_user — admin-only reads.
DROP POLICY IF EXISTS audit_events_admin ON audit_events;
CREATE POLICY audit_events_admin ON audit_events FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- verification_events (owner OR actor)
ALTER TABLE verification_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE verification_events FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS verification_events_insert ON verification_events;
CREATE POLICY verification_events_insert ON verification_events FOR INSERT TO app_user
    WITH CHECK (true);
DROP POLICY IF EXISTS verification_events_select ON verification_events;
CREATE POLICY verification_events_select ON verification_events FOR SELECT TO app_user
    USING (
        user_id  = current_setting('app.user_id', true)::integer
        OR actor_id = current_setting('app.user_id', true)::integer
    );
DROP POLICY IF EXISTS verification_events_admin ON verification_events;
CREATE POLICY verification_events_admin ON verification_events FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- attestation_attempts (subject OR assigned reviewer)
ALTER TABLE attestation_attempts ENABLE ROW LEVEL SECURITY;
ALTER TABLE attestation_attempts FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS attestation_attempts_insert ON attestation_attempts;
CREATE POLICY attestation_attempts_insert ON attestation_attempts FOR INSERT TO app_user
    WITH CHECK (true);
DROP POLICY IF EXISTS attestation_attempts_select ON attestation_attempts;
CREATE POLICY attestation_attempts_select ON attestation_attempts FOR SELECT TO app_user
    USING (
        user_id              = current_setting('app.user_id', true)::integer
        OR assigned_reviewer_1_id = current_setting('app.user_id', true)::integer
        OR assigned_reviewer_2_id = current_setting('app.user_id', true)::integer
    );
DROP POLICY IF EXISTS attestation_attempts_admin ON attestation_attempts;
CREATE POLICY attestation_attempts_admin ON attestation_attempts FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- gift_box_history
ALTER TABLE gift_box_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE gift_box_history FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS gift_box_history_insert ON gift_box_history;
CREATE POLICY gift_box_history_insert ON gift_box_history FOR INSERT TO app_user
    WITH CHECK (true);
DROP POLICY IF EXISTS gift_box_history_select ON gift_box_history;
CREATE POLICY gift_box_history_select ON gift_box_history FOR SELECT TO app_user
    USING (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS gift_box_history_admin ON gift_box_history;
CREATE POLICY gift_box_history_admin ON gift_box_history FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- business_assessment_history (business owner)
ALTER TABLE business_assessment_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE business_assessment_history FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS business_assessment_history_insert ON business_assessment_history;
CREATE POLICY business_assessment_history_insert ON business_assessment_history FOR INSERT TO app_user
    WITH CHECK (true);
DROP POLICY IF EXISTS business_assessment_history_select ON business_assessment_history;
CREATE POLICY business_assessment_history_select ON business_assessment_history FOR SELECT TO app_user
    USING (business_id IN (
        SELECT id FROM businesses
        WHERE primary_holder_id = current_setting('app.user_id', true)::integer
    ));
DROP POLICY IF EXISTS business_assessment_history_admin ON business_assessment_history;
CREATE POLICY business_assessment_history_admin ON business_assessment_history FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- platform_configuration_history (admin-only SELECT)
ALTER TABLE platform_configuration_history ENABLE ROW LEVEL SECURITY;
ALTER TABLE platform_configuration_history FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS platform_configuration_history_insert ON platform_configuration_history;
CREATE POLICY platform_configuration_history_insert ON platform_configuration_history FOR INSERT TO app_user
    WITH CHECK (true);
-- Deliberately no SELECT policy for app_user — admin-only reads.
DROP POLICY IF EXISTS platform_configuration_history_admin ON platform_configuration_history;
CREATE POLICY platform_configuration_history_admin ON platform_configuration_history FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- audit_request_log
ALTER TABLE audit_request_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_request_log FORCE  ROW LEVEL SECURITY;
DROP POLICY IF EXISTS audit_request_log_insert ON audit_request_log;
CREATE POLICY audit_request_log_insert ON audit_request_log FOR INSERT TO app_user
    WITH CHECK (true);
DROP POLICY IF EXISTS audit_request_log_select ON audit_request_log;
CREATE POLICY audit_request_log_select ON audit_request_log FOR SELECT TO app_user
    USING (user_id = current_setting('app.user_id', true)::integer);
DROP POLICY IF EXISTS audit_request_log_admin ON audit_request_log;
CREATE POLICY audit_request_log_admin ON audit_request_log FOR ALL TO app_admin USING (true) WITH CHECK (true);

-- =============================================================
-- 3h. RLS DISABLED (re-stated for explicitness)
-- The four tables below have NO ENABLE ROW LEVEL SECURITY:
--   magic_link_tokens, jwt_revocations, platform_configuration, locations
-- Access is enforced by the role grants in Section 2 and by the
-- application service layer.
-- =============================================================
