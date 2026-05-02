-- =============================================================
-- Box Fraise Identity Protocol — Reference Schema
-- BFIP v0.1.0
-- PostgreSQL reference implementation
-- =============================================================
--
-- APPEND-ONLY TABLES (protected by bf_prevent_modification trigger):
--   audit_events, verification_events, attestation_attempts,
--   gift_box_history, business_assessment_history,
--   platform_configuration_history, audit_request_log
--
-- SOFT DELETE TABLES (filter WHERE deleted_at IS NULL):
--   users, businesses
--
-- DEFERRED FK ORDER (added via ALTER TABLE after referenced tables exist):
--   1.  users.soultoken_id → soultokens
--   2.  soultokens.business_id → businesses
--   3.  soultokens.revocation_visit_id → staff_visits
--   4.  soultokens.identity_credential_id → identity_credentials
--   5.  soultokens.presence_threshold_id → presence_thresholds
--   6.  soultokens.attestation_id → visit_attestations
--   7.  soultokens surrender constraint → after staff_visits FK exists
--   8.  soultoken_renewals.triggering_presence_id → presence_events
--   9.  presence_events.session_id → presence_sessions
--   10. presence_events.box_id → visit_boxes
--   11. visit_boxes.assigned_order_id → orders
--   12. visit_attestations.presence_threshold_id → presence_thresholds
--   13. staff_visits.quality_assessment_id removed (circular FK)
--   14. presence_sessions.contributed_to_threshold_id → presence_thresholds
--
-- SEED DATA REQUIRED (see bottom of file):
--   platform_configuration initial values
--   First platform_admin user
--   First box_fraise_store location
-- =============================================================

-- =============================================================
-- SECTION 1: IDENTITY AND AUTH
-- =============================================================

-- BFIP Section 3, 4 | Core user identity record
CREATE TABLE users (
    id                              SERIAL PRIMARY KEY,
    email                           TEXT NOT NULL UNIQUE,
    apple_id                        TEXT UNIQUE,
                                    -- Apple stable user identifier
                                    -- null if registered via magic link only
    display_name                    TEXT,
    push_token                      TEXT,
    email_verified                  BOOLEAN NOT NULL DEFAULT false,
                                    -- set true on first successful magic link verification
    is_platform_admin               BOOLEAN NOT NULL DEFAULT false,
                                    -- platform-level admin flag
                                    -- distinct from staff_roles which are location-scoped
    is_banned                       BOOLEAN NOT NULL DEFAULT false,
    verification_status             TEXT NOT NULL DEFAULT 'registered'
                                    CHECK (verification_status IN (
                                        'registered',
                                        'identity_confirmed',
                                        'presence_confirmed',
                                        'attested'
                                    )),
                                    -- maps to Rust enum VerificationStatus
    attested_at                     TIMESTAMPTZ,
    soultoken_id                    INTEGER,
                                    -- FK added after soultokens created
    cleared_at                      TIMESTAMPTZ,
                                    -- when cleared status was granted
    cleared_soultoken_id            INTEGER,
                                    -- FK added after soultokens created
                                    -- separate credential from soultoken_id
                                    -- independently revocable
    platform_gift_eligible_after    TIMESTAMPTZ,
                                    -- set to gifted_at + 6 months on platform gift
                                    -- enforces one platform-covered gift per 6 months
    deleted_at                      TIMESTAMPTZ,
                                    -- soft delete — never hard delete users
    last_active_at                  TIMESTAMPTZ,
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE users IS
    'BFIP Section 13. Core user identity record. Soft-deleted via deleted_at. '
    'Filter active users with WHERE deleted_at IS NULL.';

-- BFIP Section 3.1 | Apple Sign In session records
CREATE TABLE apple_auth_sessions (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    apple_user_identifier           TEXT NOT NULL,
                                    -- stable Apple ID for this user
    identity_token_hash             TEXT NOT NULL,
                                    -- SHA-256 of Apple identity token, never plaintext
    ip_address                      TEXT,
                                    -- IP that initiated the Apple auth request
    expires_at                      TIMESTAMPTZ NOT NULL,
                                    -- session validity window
    revoked_at                      TIMESTAMPTZ,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE apple_auth_sessions IS
    'BFIP Section 3.1. Apple Sign In session audit records. '
    'identity_token_hash is SHA-256 of the Apple identity token — never stored in plaintext.';

-- BFIP Section 3.1 | Magic link tokens
CREATE TABLE magic_link_tokens (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    email                           TEXT NOT NULL,
    token_hash                      TEXT NOT NULL UNIQUE,
                                    -- SHA-256 of actual token, never plaintext
    ip_address                      TEXT,
                                    -- IP that requested this token
    rate_limit_key                  TEXT,
                                    -- rate limit key used for this request
    request_number_in_window        INTEGER,
                                    -- nth token issued to this email in current window
                                    -- makes rate limit auditable without Redis
    expires_at                      TIMESTAMPTZ NOT NULL,
    used_at                         TIMESTAMPTZ,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE magic_link_tokens IS
    'BFIP Section 3.1. Magic link authentication tokens. '
    'token_hash is SHA-256 — never store the raw token. '
    'rate_limit_key and request_number_in_window make rate limiting auditable.';

-- JWT revocation list — prunable after expires_at
CREATE TABLE jwt_revocations (
    id                              SERIAL PRIMARY KEY,
    jti                             TEXT NOT NULL UNIQUE,
                                    -- JWT ID claim from the revoked token
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    revoked_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at                      TIMESTAMPTZ NOT NULL
                                    -- prune rows WHERE expires_at < now()
                                    -- scheduled daily
);

COMMENT ON TABLE jwt_revocations IS
    'JWT revocation list. Rows are prunable after expires_at passes. '
    'Run: DELETE FROM jwt_revocations WHERE expires_at < now() daily.';

-- BFIP Section 3 | Identity credential verification records
CREATE TABLE identity_credentials (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    credential_type                 TEXT NOT NULL
                                    CHECK (credential_type IN (
                                        'stripe_identity',
                                        'mdl',
                                        'eidas'
                                    )),
                                    -- credential_type agnostic for mDL and eIDAS expansion
                                    -- maps to Rust enum IdentityCredentialType
    external_session_id             TEXT,
                                    -- Stripe session ID or equivalent
    stripe_identity_report_id       TEXT,
                                    -- Stripe verification report ID for audit
    raw_verification_status         TEXT,
                                    -- Stripe status: verified / requires_input / processing
    response_hash                   TEXT,
                                    -- HMAC-SHA256 of raw Stripe webhook payload
                                    -- proves stored status matches what Stripe sent
    cooling_app_opens_required      INTEGER NOT NULL DEFAULT 3,
                                    -- stored per-credential so historical records
                                    -- remain auditable if threshold changes
    verified_at                     TIMESTAMPTZ NOT NULL,
    cooling_ends_at                 TIMESTAMPTZ NOT NULL,
                                    -- 7 days from verified_at
    cooling_completed_at            TIMESTAMPTZ,
                                    -- set when all cooling requirements met
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE identity_credentials IS
    'BFIP Section 3. Identity confirmation records. '
    'response_hash proves the stored verification status was not tampered with after receipt. '
    'cooling_app_opens_required stored per-record for historical auditability.';

-- BFIP Section 3b | Background check records
CREATE TABLE background_checks (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    identity_credential_id          INTEGER NOT NULL
                                    REFERENCES identity_credentials(id),
                                    -- background check uses identity confirmed by Stripe
    provider                        TEXT NOT NULL
                                    CHECK (provider IN (
                                        'comply_advantage',
                                        'refinitiv',
                                        'lexisnexis',
                                        'socure'
                                    )),
    check_type                      TEXT NOT NULL
                                    CHECK (check_type IN (
                                        'sanctions',
                                        'identity_fraud',
                                        'criminal'
                                    )),
                                    -- sanctions + identity_fraud required for attested
                                    -- criminal required for cleared (optional elevation)
                                    -- adverse_media reserved for future versions
    external_check_id               TEXT,
                                    -- provider-assigned check identifier for audit
    status                          TEXT NOT NULL DEFAULT 'pending'
                                    CHECK (status IN (
                                        'pending',
                                        'passed',
                                        'failed',
                                        'review_required',
                                        'expired'
                                    )),
    response_hash                   TEXT,
                                    -- HMAC-SHA256 of raw provider response
                                    -- proves stored result was not tampered with
    checked_at                      TIMESTAMPTZ,
                                    -- when provider returned result
    expires_at                      TIMESTAMPTZ,
                                    -- checks have validity period (default 12 months)
                                    -- expired checks must be re-run before proceeding
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE background_checks IS
    'BFIP Section 3b. Background check records — sanctions and identity fraud screening. '
    'Initiated after Stage 1 identity confirmation, must pass before Stage 2 cooling period. '
    'Box Fraise stores pass/fail signal only — never underlying identity data. '
    'response_hash proves stored result matches provider response. '
    'Two checks required per user: sanctions AND identity_fraud. '
    'criminal and adverse_media check types reserved for future protocol versions.';


CREATE TABLE cooling_period_events (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    credential_id                   INTEGER NOT NULL
                                    REFERENCES identity_credentials(id),
    event_type                      TEXT NOT NULL DEFAULT 'app_open'
                                    CHECK (event_type IN ('app_open')),
    device_identifier               TEXT,
                                    -- App Attest device identifier
    app_attest_assertion            TEXT,
                                    -- Apple App Attest proof that this is a genuine iOS device
    calendar_date                   DATE NOT NULL,
                                    -- enforces separate-day requirement
    occurred_at                     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, credential_id, calendar_date)
                                    -- one qualifying event per day per credential
);

COMMENT ON TABLE cooling_period_events IS
    'BFIP Section 4.3. App opens during the cooling period. '
    'UNIQUE constraint enforces one event per calendar day per credential. '
    'device_identifier and app_attest_assertion prove genuine iOS device ownership.';

-- =============================================================
-- SECTION 2: SOULTOKENS
-- =============================================================

-- BFIP Section 7 | Non-transferable verified identity credentials
CREATE TABLE soultokens (
    id                              SERIAL PRIMARY KEY,
    uuid                            UUID NOT NULL UNIQUE
                                    DEFAULT gen_random_uuid(),
                                    -- internal identifier, NEVER exposed externally
                                    -- used for all internal database references
    display_code                    TEXT NOT NULL UNIQUE
                                    CHECK (display_code ~ '^[A-Z0-9]{4}-[A-Z0-9]{4}-[A-Z0-9]{4}$'),
                                    -- HMAC-derived from UUID at issuance
                                    -- see reference/cryptography.md for derivation
                                    -- what users and third parties reference
                                    -- admin sees "FRS-" + first 8 chars of UUID at display time
                                    -- "FRS-" prefix is NEVER stored
    display_code_key_version        INTEGER NOT NULL DEFAULT 1,
                                    -- HMAC key version used for display code derivation
                                    -- enables key rotation without invalidating existing codes
    schema_version                  INTEGER NOT NULL DEFAULT 1,
                                    -- BFIP protocol schema version at issuance
    holder_user_id                  INTEGER NOT NULL REFERENCES users(id),
    token_type                      TEXT NOT NULL
                                    CHECK (token_type IN (
                                        'user',
                                        'business',
                                        'cleared'
                                    )),
                                    -- user: standard attested soultoken
                                    -- business: business soultoken
                                    -- cleared: optional elevated status soultoken
    attested_soultoken_id           INTEGER,
                                    -- FK added after soultokens self-references resolve
                                    -- populated when token_type = 'cleared'
                                    -- references the attested soultoken that qualified this user
    CONSTRAINT cleared_soultoken_requires_attested CHECK (
        token_type != 'cleared' OR attested_soultoken_id IS NOT NULL
    ),
    business_id                     INTEGER,
                                    -- FK added after businesses created
                                    -- NOT NULL when token_type = 'business'
    CONSTRAINT business_soultoken_requires_business_id CHECK (
        token_type != 'business' OR business_id IS NOT NULL
    ),
    -- Verification chain — all three required for user soultokens
    identity_credential_id          INTEGER,
                                    -- FK added after identity_credentials verified
    presence_threshold_id           INTEGER,
                                    -- FK added after presence_thresholds created
    attestation_id                  INTEGER,
                                    -- FK added after visit_attestations created
    CONSTRAINT user_soultoken_requires_credential CHECK (
        token_type != 'user' OR identity_credential_id IS NOT NULL
    ),
    CONSTRAINT user_soultoken_requires_threshold CHECK (
        token_type != 'user' OR presence_threshold_id IS NOT NULL
    ),
    CONSTRAINT user_soultoken_requires_attestation CHECK (
        token_type != 'user' OR attestation_id IS NOT NULL
    ),
    vc_credential_json              JSONB,
                                    -- W3C Verifiable Credential JSON
                                    -- nullable in v0.1.0, populated in v1.0.0
    signature                       TEXT,
                                    -- Ed25519 signature over:
                                    -- uuid || holder_user_id || issued_at ||
                                    -- expires_at || display_code
                                    -- expires_at is signed to prevent DB-level extension
                                    -- see reference/cryptography.md
    issued_at                       TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at                      TIMESTAMPTZ NOT NULL
                                    DEFAULT now() + INTERVAL '12 months',
    last_renewed_at                 TIMESTAMPTZ,
    revoked_at                      TIMESTAMPTZ,
    revocation_reason               TEXT
                                    CHECK (revocation_reason IN (
                                        'stripe_flag',
                                        'staff_rescission',
                                        'platform_ban',
                                        'voluntary_surrender'
                                    )),
    revocation_staff_id             INTEGER REFERENCES users(id),
    revocation_visit_id             INTEGER,
                                    -- FK added after staff_visits created
                                    -- required for voluntary_surrender
    surrender_witnessed_by          INTEGER REFERENCES users(id),
                                    -- Box Fraise staff who witnessed voluntary surrender
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE soultokens IS
    'BFIP Section 7. Non-transferable verified identity credentials. '
    'uuid is internal only — never expose in API responses. '
    'display_code is what users and third parties reference. '
    'Admin reference "FRS-{first8charsOfUUID}" is generated at display time, never stored. '
    'signature covers uuid||holder_user_id||issued_at||expires_at||display_code via Ed25519.';

COMMENT ON COLUMN soultokens.uuid IS
    'Internal identifier. UUID v4. NEVER exposed externally. '
    'Used for all internal FK references.';

COMMENT ON COLUMN soultokens.display_code IS
    'Human-readable external identifier. HMAC-SHA256 derived from uuid. '
    'Derivation: Base36(HMAC-SHA256(display_code_key, uuid_bytes)[0:9]) formatted as XXXX-XXXX-XXXX. '
    'See reference/cryptography.md for exact algorithm.';

COMMENT ON COLUMN soultokens.signature IS
    'Ed25519 signature over: uuid || "|" || holder_user_id || "|" || '
    'issued_at_iso8601 || "|" || expires_at_iso8601 || "|" || display_code. '
    'expires_at is included to prevent database-level validity extension. '
    'See reference/cryptography.md for exact format.';

-- Deferred FK from users to soultokens
ALTER TABLE users
    ADD CONSTRAINT users_soultoken_fk
    FOREIGN KEY (soultoken_id) REFERENCES soultokens(id);

-- BFIP Section 7.6 | Soultoken renewal records
CREATE TABLE soultoken_renewals (
    id                              SERIAL PRIMARY KEY,
    soultoken_id                    INTEGER NOT NULL REFERENCES soultokens(id),
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    triggering_presence_id          INTEGER,
                                    -- FK added after presence_events created
    renewal_type                    TEXT NOT NULL
                                    CHECK (renewal_type IN (
                                        'beacon_dwell',
                                        'nfc_tap'
                                    )),
    previous_expires_at             TIMESTAMPTZ NOT NULL,
    new_expires_at                  TIMESTAMPTZ NOT NULL,
    renewed_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE soultoken_renewals IS
    'BFIP Section 7.6. Soultoken renewal history. '
    'One qualifying presence event renews for 12 months. '
    'Revoked soultokens cannot be renewed (enforced by bf_prevent_revoked_soultoken_renewal trigger).';

-- =============================================================
-- SECTION 3: PHYSICAL INFRASTRUCTURE
-- =============================================================

-- BFIP Section 10, 12 | Physical locations
CREATE TABLE locations (
    id                              SERIAL PRIMARY KEY,
    name                            TEXT NOT NULL,
    location_type                   TEXT NOT NULL
                                    CHECK (location_type IN (
                                        'box_fraise_store',
                                        'partner_business'
                                    )),
                                    -- box_fraise_store: platform-owned, elevated staff authority
                                    -- partner_business: third-party, standard authority
    address                         TEXT NOT NULL,
    latitude                        NUMERIC(9,6),
    longitude                       NUMERIC(9,6),
    timezone                        TEXT NOT NULL DEFAULT 'America/Edmonton',
                                    -- IANA timezone for delivery window scheduling
    contact_email                   TEXT,
    contact_phone                   TEXT,
    google_place_id                 TEXT,
                                    -- for map integration and address validation
    is_active                       BOOLEAN NOT NULL DEFAULT true,
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE locations IS
    'BFIP Section 10, 12. Physical locations — Box Fraise stores and partner businesses. '
    'box_fraise_store locations have elevated staff authority for attestation and support. '
    'timezone is used for delivery window scheduling and calendar day calculations.';

-- BFIP Section 12 | Partner businesses on the platform
CREATE TABLE businesses (
    id                              SERIAL PRIMARY KEY,
    location_id                     INTEGER NOT NULL REFERENCES locations(id),
    soultoken_id                    INTEGER REFERENCES soultokens(id),
                                    -- business soultoken
    primary_holder_id               INTEGER NOT NULL REFERENCES users(id),
                                    -- must be attested user at time of creation
    primary_holder_soultoken_id     INTEGER REFERENCES soultokens(id),
                                    -- soultoken status at business creation time
                                    -- preserved even if user soultoken later revoked
    stripe_customer_id              TEXT UNIQUE,
                                    -- Stripe customer ID for platform fee billing
    name                            TEXT NOT NULL,
    verification_status             TEXT NOT NULL DEFAULT 'pending'
                                    CHECK (verification_status IN (
                                        'pending',
                                        'active',
                                        'suspended'
                                    )),
    beacon_suspended                BOOLEAN NOT NULL DEFAULT false,
                                    -- true after 3 failed quality assessments in 12 months
    beacon_suspended_at             TIMESTAMPTZ,
                                    -- when beacon_suspended was set to true
    suspended_at                    TIMESTAMPTZ,
                                    -- when full business suspension occurred
    onboarded_at                    TIMESTAMPTZ,
                                    -- when verification_status became 'active'
    is_active                       BOOLEAN NOT NULL DEFAULT true,
    platform_fee_cents              INTEGER NOT NULL DEFAULT 0,
    deleted_at                      TIMESTAMPTZ,
                                    -- soft delete
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE businesses IS
    'BFIP Section 12. Partner businesses. '
    'Creation requires an attested user as primary_holder. '
    'approaching_suspension (2 failed assessments) is DERIVED from business_assessment_history '
    'not stored — query: SELECT COUNT(*) FROM business_assessment_history '
    'WHERE business_id = $1 AND passed = false AND assessed_at > now() - INTERVAL 12 months. '
    'Soft-deleted via deleted_at.';

-- Deferred FK from soultokens to businesses
ALTER TABLE soultokens
    ADD CONSTRAINT soultokens_business_fk
    FOREIGN KEY (business_id) REFERENCES businesses(id);

-- BFIP Section 8 | BLE beacons at business locations
CREATE TABLE beacons (
    id                              SERIAL PRIMARY KEY,
    location_id                     INTEGER NOT NULL REFERENCES locations(id),
    business_id                     INTEGER REFERENCES businesses(id),
    secret_key                      TEXT NOT NULL,
                                    -- HMAC secret for daily UUID derivation
                                    -- derivation: HMAC-SHA256(secret_key, business_id||":"||YYYY-MM-DD)
                                    -- see reference/cryptography.md
    previous_secret_key             TEXT,
                                    -- preserved for 24-hour grace period during key rotation
    key_rotated_at                  TIMESTAMPTZ,
                                    -- when secret_key was last rotated
    hardware_key_id                 TEXT,
                                    -- nullable in v0.1.0
                                    -- populated when proprietary hardware ships (v0.2.x)
    minimum_rssi_threshold          INTEGER NOT NULL DEFAULT -70,
                                    -- minimum signal strength in dBm for qualifying events
                                    -- configurable per beacon (indoor vs outdoor)
    is_active                       BOOLEAN NOT NULL DEFAULT true,
    last_seen_at                    TIMESTAMPTZ,
    last_rotation_at                TIMESTAMPTZ,
    failure_count                   INTEGER NOT NULL DEFAULT 0,
                                    -- consecutive rotation/health failures
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE beacons IS
    'BFIP Section 8. BLE beacons at registered business locations. '
    'Daily UUID derivation: HMAC-SHA256(secret_key, business_id_string||":"||"YYYY-MM-DD" in UTC). '
    'Key rotation: new key in secret_key, old key in previous_secret_key, valid for 24hr grace period. '
    'minimum_rssi_threshold configurable per beacon — indoor locations may need higher threshold.';

COMMENT ON COLUMN beacons.secret_key IS
    'HMAC-SHA256 secret for daily UUID derivation. '
    'Derivation formula: HMAC-SHA256(secret_key, business_id_str || ":" || "YYYY-MM-DD") '
    'where date is current UTC date. Output formatted as UUID v4 string. '
    'See reference/cryptography.md Section 1 for exact specification.';

-- BFIP Section 8.1 | Daily beacon UUID rotation records
CREATE TABLE beacon_rotation_log (
    id                              SERIAL PRIMARY KEY,
    beacon_id                       INTEGER NOT NULL REFERENCES beacons(id),
    calendar_date                   DATE NOT NULL,
    expected_uuid_hash              TEXT NOT NULL,
                                    -- SHA-256 of the expected UUID for this date
                                    -- UUID itself never stored — derive at validation time
    first_seen_at                   TIMESTAMPTZ,
    rotation_status                 TEXT NOT NULL DEFAULT 'active'
                                    CHECK (rotation_status IN (
                                        'active',
                                        'expired',
                                        'failed'
                                    )),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (beacon_id, calendar_date)
);

COMMENT ON TABLE beacon_rotation_log IS
    'BFIP Section 8.1. Daily beacon UUID rotation records. '
    'expected_uuid_hash is SHA-256 of the expected UUID — UUID itself never stored. '
    'Derive expected UUID at validation time, hash it, compare to stored hash.';

-- BFIP Section 8.4 | Beacon health monitoring
CREATE TABLE beacon_health_log (
    id                              SERIAL PRIMARY KEY,
    beacon_id                       INTEGER NOT NULL REFERENCES beacons(id),
    checked_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    is_responding                   BOOLEAN NOT NULL,
    signal_strength                 INTEGER,
    firmware_version                TEXT
                                    -- nullable in v0.1.0
                                    -- populated when proprietary hardware ships (v0.2.x)
);

COMMENT ON TABLE beacon_health_log IS
    'BFIP Section 8.4. Periodic beacon health check records. '
    'firmware_version is nullable in v0.1.0 — reserved for hardware extension (v0.2.x).';

-- =============================================================
-- SECTION 4: STAFF
-- =============================================================

-- BFIP Section 6.1, 10 | Box Fraise employee authority levels
CREATE TABLE staff_roles (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    location_id                     INTEGER REFERENCES locations(id),
                                    -- null means platform-wide authority
                                    -- delivery_staff MUST have location_id (see constraint)
    role                            TEXT NOT NULL
                                    CHECK (role IN (
                                        'delivery_staff',
                                        'attestation_reviewer',
                                        'platform_admin'
                                    )),
                                    -- delivery_staff: field ops, attestations, support
                                    -- attestation_reviewer: remote co-signing only
                                    -- platform_admin: full system authority
    granted_by                      INTEGER NOT NULL REFERENCES users(id),
                                    -- platform admin who granted this role
    confirmed_by                    INTEGER REFERENCES users(id),
                                    -- second admin confirmation — required for platform_admin grants
    confirmed_at                    TIMESTAMPTZ,
    CONSTRAINT delivery_staff_requires_location CHECK (
        role != 'delivery_staff' OR location_id IS NOT NULL
    ),
    CONSTRAINT no_self_confirmation CHECK (
        confirmed_by IS NULL
        OR (confirmed_by != user_id AND confirmed_by != granted_by)
    ),
    expires_at                      TIMESTAMPTZ,
                                    -- for contract-based roles with defined end dates
    granted_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at                      TIMESTAMPTZ
);

COMMENT ON TABLE staff_roles IS
    'BFIP Section 6.1, 10. Box Fraise employee authority levels. '
    'delivery_staff must have location_id — they are location-specific. '
    'platform_admin grants require confirmed_by from a second admin (two-person rule). '
    'no_self_confirmation prevents self-confirmation of role grants. '
    'Active roles: WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()).';

-- BFIP Section 6.5 | Reviewer assignment audit log
CREATE TABLE reviewer_assignment_log (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL,
                                    -- FK added after staff_visits created
    reviewer_id                     INTEGER NOT NULL REFERENCES users(id),
    assigned_at                     TIMESTAMPTZ NOT NULL DEFAULT now(),
    assignment_algorithm_version    TEXT NOT NULL,
                                    -- v1: round-robin with collusion constraints
                                    -- see PROTOCOL.md Section 6.5 for algorithm spec
    collusion_check_passed          BOOLEAN NOT NULL,
    collusion_check_details         JSONB NOT NULL DEFAULT '{}',
                                    -- records which constraints were evaluated
                                    -- e.g. {"same_location_30d": false, "cosign_7d": 2}
    recent_cosign_count             INTEGER NOT NULL DEFAULT 0,
                                    -- how many times this reviewer has co-signed
                                    -- with the other assigned reviewer in last 7 days
    UNIQUE (visit_id, reviewer_id)
);

COMMENT ON TABLE reviewer_assignment_log IS
    'BFIP Section 6.5. Reviewer assignment audit records. '
    'Assignment algorithm v1: round-robin excluding reviewers who worked at same location '
    'as delivery staff in last 30 days, or co-signed with other reviewer >3 times in 7 days. '
    'collusion_check_details records full constraint evaluation for audit.';

-- =============================================================
-- SECTION 5: STAFF VISITS
-- =============================================================

-- BFIP Section 10 | Unified Box Fraise employee visit records
CREATE TABLE staff_visits (
    id                              SERIAL PRIMARY KEY,
    location_id                     INTEGER NOT NULL REFERENCES locations(id),
    staff_id                        INTEGER NOT NULL REFERENCES users(id),
    visit_type                      TEXT NOT NULL
                                    CHECK (visit_type IN (
                                        'delivery',
                                        'support',
                                        'quality',
                                        'combined'
                                    )),
    status                          TEXT NOT NULL DEFAULT 'scheduled'
                                    CHECK (status IN (
                                        'scheduled',
                                        'in_progress',
                                        'completed',
                                        'cancelled'
                                    )),
    scheduled_at                    TIMESTAMPTZ NOT NULL,
    window_hours                    INTEGER NOT NULL DEFAULT 4,
                                    -- delivery window duration in hours
    support_booking_capacity        INTEGER NOT NULL DEFAULT 0,
                                    -- max support bookings for this visit
    business_notified_at            TIMESTAMPTZ,
                                    -- when business received 4-hour window notification
                                    -- business gets window only, NOT exact time
    staff_revealed_at               TIMESTAMPTZ,
                                    -- when exact schedule revealed to delivery staff
                                    -- revealed 2 hours before window ONLY
    arrived_at                      TIMESTAMPTZ,
    arrived_latitude                NUMERIC(9,6),
                                    -- GPS at arrival stored separately for queryability
    arrived_longitude               NUMERIC(9,6),
    departed_at                     TIMESTAMPTZ,
    cancelled_at                    TIMESTAMPTZ,
    cancellation_reason             TEXT,
    expected_box_count              INTEGER NOT NULL DEFAULT 0,
    actual_box_count                INTEGER,
                                    -- recorded at departure
    delivery_signature              TEXT,
                                    -- Secure Enclave signature from delivery staff
                                    -- covers evidence_hash + location + timestamp
    evidence_hash                   TEXT,
                                    -- SHA-256 of photo_hash || gps_json || beacon_witness_hmac
                                    -- see reference/cryptography.md Section 5
    evidence_storage_uri            TEXT,
                                    -- URI of evidence package in secure storage
                                    -- hash proves integrity, URI proves location
    route_proof                     TEXT,
                                    -- nullable in v0.1.0
                                    -- GPS-signed delivery route for future chain of custody
    gift_box_covered                BOOLEAN NOT NULL DEFAULT false,
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE staff_visits IS
    'BFIP Section 10. Unified Box Fraise employee visit records. '
    'Every visit — delivery, support, quality, or combined — uses this table. '
    'Every visit is a potential attestation opportunity. '
    'Schedule security: business_notified_at is 4-hour window only. '
    'staff_revealed_at is exact time, revealed 2 hours before window only. '
    'evidence_hash covers photo + GPS + beacon HMAC. Reviewers sign this hash.';

-- Deferred FK from soultokens to staff_visits
ALTER TABLE soultokens
    ADD CONSTRAINT soultokens_revocation_visit_fk
    FOREIGN KEY (revocation_visit_id) REFERENCES staff_visits(id);

-- Add surrender constraint now that staff_visits FK exists
ALTER TABLE soultokens
    ADD CONSTRAINT surrender_requires_visit CHECK (
        revocation_reason != 'voluntary_surrender'
        OR revocation_visit_id IS NOT NULL
    );

-- Deferred FK from reviewer_assignment_log to staff_visits
ALTER TABLE reviewer_assignment_log
    ADD CONSTRAINT reviewer_assignment_log_visit_fk
    FOREIGN KEY (visit_id) REFERENCES staff_visits(id);

-- BFIP Section 10.2 | Visit notification records
CREATE TABLE staff_visit_notifications (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    recipient_type                  TEXT NOT NULL
                                    CHECK (recipient_type IN (
                                        'business',
                                        'staff'
                                    )),
    recipient_id                    INTEGER NOT NULL REFERENCES users(id),
    notification_type               TEXT NOT NULL
                                    CHECK (notification_type IN (
                                        'window_notification',
                                        -- sent to business: 4-hour window, no exact time
                                        'schedule_reveal'
                                        -- sent to staff: exact time, 2 hours before
                                    )),
    channel                         TEXT NOT NULL
                                    CHECK (channel IN (
                                        'push',
                                        'email'
                                    )),
    content                         TEXT NOT NULL,
                                    -- what was communicated — proves what recipient was told
    sent_at                         TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at                    TIMESTAMPTZ,
    read_at                         TIMESTAMPTZ
);

COMMENT ON TABLE staff_visit_notifications IS
    'BFIP Section 10.2. Visit notification records. '
    'window_notification: sent to business with 4-hour window, no exact time. '
    'schedule_reveal: sent to staff with exact time, 2 hours before window only. '
    'content field proves what was communicated to recipient.';

-- BFIP Section 6.6, 10.4 | Multi-signature attestation co-signs
CREATE TABLE visit_signatures (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    reviewer_id                     INTEGER NOT NULL REFERENCES users(id),
    signature                       TEXT NOT NULL,
                                    -- Secure Enclave signature via Face ID
                                    -- signs evidence_hash_reviewed
    evidence_hash_reviewed          TEXT NOT NULL,
                                    -- the specific evidence hash the reviewer signed
                                    -- proves reviewer evaluated specific unmodified evidence
    assigned_at                     TIMESTAMPTZ NOT NULL,
                                    -- when reviewer was assigned (before delivery)
    deadline                        TIMESTAMPTZ NOT NULL,
                                    -- 48 hours from visit arrival
    signed_at                       TIMESTAMPTZ,
    missed_at                       TIMESTAMPTZ,
                                    -- set when system processes missed deadline
    deadline_enforced_at            TIMESTAMPTZ,
                                    -- set when deadline processing runs
                                    -- prevents double-processing of missed deadlines
    reassigned_reviewer_id          INTEGER REFERENCES users(id),
                                    -- set if reviewer replaced after missed deadline
    UNIQUE (visit_id, reviewer_id)
                                    -- one signature per reviewer per visit
);

COMMENT ON TABLE visit_signatures IS
    'BFIP Section 6.6, 10.4. Reviewer co-signatures for staff visit attestations. '
    'Two reviewers required per visit — enforced at application layer + reviewer_assignment_log. '
    'Reviewers sign evidence_hash_reviewed via Face ID (Secure Enclave binding). '
    'deadline_enforced_at prevents double-processing of missed deadlines.';

-- BFIP Section 12.3 | Structured business quality assessments
CREATE TABLE quality_assessments (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    business_id                     INTEGER NOT NULL REFERENCES businesses(id),
    assessor_id                     INTEGER NOT NULL REFERENCES users(id),
    beacon_functioning              BOOLEAN NOT NULL,
    staff_performing_correctly      BOOLEAN NOT NULL,
    standards_maintained            BOOLEAN NOT NULL,
    overall_pass                    BOOLEAN NOT NULL,
    follow_up_required              BOOLEAN NOT NULL DEFAULT false,
    follow_up_visit_id              INTEGER REFERENCES staff_visits(id),
    notes                           TEXT,
    assessed_at                     TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE quality_assessments IS
    'BFIP Section 12.3. Structured quality assessment per staff visit. '
    'Distinct from business_assessment_history which tracks rolling 12-month counts. '
    'This table has assessment detail. History table has running counts for window queries.';

-- BFIP Section 12.3, 12.4 | Rolling assessment history for 12-month window
CREATE TABLE business_assessment_history (
    id                              SERIAL PRIMARY KEY,
    business_id                     INTEGER NOT NULL REFERENCES businesses(id),
    assessment_id                   INTEGER NOT NULL
                                    REFERENCES quality_assessments(id),
    beacon_id                       INTEGER REFERENCES beacons(id),
                                    -- which beacon was assessed
                                    -- nullable for business-level assessments
    passed                          BOOLEAN NOT NULL,
    failed_count_at_time            INTEGER NOT NULL,
                                    -- running failed count at time of this record
                                    -- proves at what point suspension was triggered
    assessed_at                     TIMESTAMPTZ NOT NULL DEFAULT now()
                                    -- indexed for 12-month rolling window queries
);

COMMENT ON TABLE business_assessment_history IS
    'BFIP Section 12.3, 12.4. Append-only rolling assessment history. '
    'Distinct from quality_assessments which has assessment detail. '
    'This table is optimised for 12-month rolling window queries: '
    'SELECT COUNT(*) WHERE business_id = $1 AND passed = false '
    'AND assessed_at > now() - INTERVAL 12 months. '
    'failed_count_at_time proves at what point beacon suspension was triggered.';

-- =============================================================
-- SECTION 6: VERIFICATION PROTOCOL
-- =============================================================

-- BFIP Section 5.6 | Presence session records
CREATE TABLE presence_sessions (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    business_id                     INTEGER NOT NULL REFERENCES businesses(id),
    beacon_id                       INTEGER REFERENCES beacons(id),
    visit_id                        INTEGER REFERENCES staff_visits(id),
                                    -- null for sessions outside delivery windows
    device_identifier               TEXT,
                                    -- App Attest device identifier
                                    -- session bound to one device
    device_attestation_verified     BOOLEAN NOT NULL DEFAULT false,
                                    -- true if App Attest assertion was validated
    device_attestation_verified_at  TIMESTAMPTZ,
    started_at                      TIMESTAMPTZ NOT NULL,
    ended_at                        TIMESTAMPTZ,
    total_dwell_minutes             INTEGER,
    is_qualifying                   BOOLEAN NOT NULL DEFAULT false,
                                    -- true if session meets all threshold requirements
    rejection_reason                TEXT,
                                    -- why session did not qualify if is_qualifying = false
    contributed_to_threshold_id     INTEGER,
                                    -- FK added after presence_thresholds created
                                    -- set when this session produces a qualifying event
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE presence_sessions IS
    'BFIP Section 5.6. Presence sessions group multiple beacon pings into one dwell period. '
    'Sessions are bound to a single device via device_identifier (App Attest). '
    'is_qualifying set when total_dwell_minutes >= 15 and RSSI threshold met. '
    'contributed_to_threshold_id links qualifying sessions to the threshold they advanced.';

-- BFIP Section 5.3 | Individual presence events
CREATE TABLE presence_events (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    business_id                     INTEGER NOT NULL REFERENCES businesses(id),
                                    -- denormalised for query performance
                                    -- derived from session but stored directly
    beacon_id                       INTEGER REFERENCES beacons(id),
                                    -- denormalised for query performance
    session_id                      INTEGER,
                                    -- FK added after presence_sessions created
    box_id                          INTEGER,
                                    -- FK added after visit_boxes created
    event_type                      TEXT NOT NULL
                                    CHECK (event_type IN (
                                        'beacon_dwell',
                                        'nfc_tap'
                                    )),
    rssi                            INTEGER,
                                    -- received signal strength in dBm (beacon_dwell only)
    rssi_threshold_applied          INTEGER,
                                    -- minimum threshold at time of event
                                    -- stored to prove enforcement even if threshold changes
    dwell_start_at                  TIMESTAMPTZ,
                                    -- when continuous dwell began (beacon_dwell only)
    dwell_end_at                    TIMESTAMPTZ,
                                    -- when continuous dwell ended (beacon_dwell only)
    dwell_minutes                   INTEGER,
                                    -- computed dwell duration
    is_qualifying                   BOOLEAN NOT NULL DEFAULT false,
                                    -- true if meets all threshold requirements
    rejection_reason                TEXT,
                                    -- why event did not qualify if is_qualifying = false
    app_attest_assertion            TEXT,
                                    -- Apple App Attest proof of genuine iOS device
    beacon_witness_hmac             TEXT,
                                    -- HMAC proving event at this beacon on this day for this user
                                    -- see reference/cryptography.md Section 2
    hardware_identifier             TEXT,
                                    -- App Attest device ID in v0.1.0
                                    -- proprietary hardware ID in v0.2.x
    calendar_date                   DATE NOT NULL DEFAULT CURRENT_DATE,
                                    -- for separate-day threshold enforcement
    occurred_at                     TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE presence_events IS
    'BFIP Section 5.3. Individual presence events — beacon dwells and NFC taps. '
    'business_id and beacon_id are denormalised for query performance. '
    'rssi_threshold_applied records the threshold at event time — '
    'proves enforcement even if threshold is later changed. '
    'beacon_witness_hmac proves event at specific beacon for specific user on specific day. '
    'See reference/cryptography.md Section 2 for beacon_witness_hmac derivation.';

-- Deferred FKs for presence_events
ALTER TABLE presence_events
    ADD CONSTRAINT presence_events_session_fk
    FOREIGN KEY (session_id) REFERENCES presence_sessions(id);

ALTER TABLE soultoken_renewals
    ADD CONSTRAINT soultoken_renewals_presence_fk
    FOREIGN KEY (triggering_presence_id) REFERENCES presence_events(id);

-- BFIP Section 5.4, 5.5 | Presence threshold tracking per user
CREATE TABLE presence_thresholds (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL UNIQUE,
                                    -- one threshold record per user
    business_id                     INTEGER NOT NULL REFERENCES businesses(id),
                                    -- single business only — no fallback, no multi-business path
                                    -- if business loses beacon privileges, reset and reassign
    event_count                     INTEGER NOT NULL DEFAULT 0,
                                    -- qualifying events at this business
    days_count                      INTEGER NOT NULL DEFAULT 0,
                                    -- separate calendar days with qualifying events
    started_at                      TIMESTAMPTZ,
                                    -- when user began presence verification at this business
    last_qualifying_event_at        TIMESTAMPTZ,
                                    -- for detecting abandoned verification attempts
    threshold_met_at                TIMESTAMPTZ,
                                    -- when presence_confirmed status was reached
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE presence_thresholds IS
    'BFIP Section 5.4, 5.5. Presence threshold tracking. '
    'Single business only — no fallback path. '
    'If assigned business loses beacon privileges before threshold met: '
    'reset event_count and days_count, reassign to new business. No count preserved. '
    'Threshold met when event_count >= 3 AND days_count >= 3.';

-- Deferred FKs to presence_thresholds
ALTER TABLE soultokens
    ADD CONSTRAINT soultokens_presence_threshold_fk
    FOREIGN KEY (presence_threshold_id) REFERENCES presence_thresholds(id);

ALTER TABLE presence_sessions
    ADD CONSTRAINT presence_sessions_threshold_fk
    FOREIGN KEY (contributed_to_threshold_id) REFERENCES presence_thresholds(id);

-- BFIP Section 5.5 | Junction table linking qualifying events to thresholds
CREATE TABLE qualifying_presence_events (
    id                              SERIAL PRIMARY KEY,
    threshold_id                    INTEGER NOT NULL REFERENCES presence_thresholds(id),
    presence_event_id               INTEGER NOT NULL REFERENCES presence_events(id),
    added_at                        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (threshold_id, presence_event_id)
);

COMMENT ON TABLE qualifying_presence_events IS
    'BFIP Section 5.5. Junction table recording exactly which presence events '
    'counted toward a user''s presence threshold. '
    'Populated when threshold_met_at is set. Provides direct auditability '
    'of which events were the qualifying 3.';

-- BFIP Section 6 | Staff attestation records
CREATE TABLE visit_attestations (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    staff_id                        INTEGER NOT NULL REFERENCES users(id),
    presence_threshold_id           INTEGER NOT NULL REFERENCES presence_thresholds(id),
                                    -- proves which presence threshold qualified this user
    assigned_reviewer_1_id          INTEGER NOT NULL REFERENCES users(id),
    assigned_reviewer_2_id          INTEGER NOT NULL REFERENCES users(id),
    CONSTRAINT reviewers_distinct CHECK (
        assigned_reviewer_1_id != assigned_reviewer_2_id
        AND assigned_reviewer_1_id != staff_id
        AND assigned_reviewer_2_id != staff_id
    ),
    user_present_confirmed          BOOLEAN NOT NULL DEFAULT false,
                                    -- staff confirmed user physically present
    user_identity_verified_at       TIMESTAMPTZ,
                                    -- timestamp when staff confirmed user identity
    location_confirmed              BOOLEAN NOT NULL DEFAULT false,
                                    -- attestation happened at correct business location
    photo_hash                      TEXT,
                                    -- SHA-256 of photo taken during attestation
                                    -- specific to this user, separate from visit evidence hash
    photo_storage_uri               TEXT,
                                    -- URI of photo in secure storage
    staff_signature                 TEXT,
                                    -- Secure Enclave signature from delivery staff
    co_sign_deadline                TIMESTAMPTZ,
                                    -- 48 hours from staff_signature
    status                          TEXT NOT NULL DEFAULT 'pending'
                                    CHECK (status IN (
                                        'pending',
                                        'co_sign_pending',
                                        'approved',
                                        'rejected'
                                    )),
    attempt_number                  INTEGER NOT NULL DEFAULT 1,
                                    -- increments on each rejection and retry
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE visit_attestations IS
    'BFIP Section 6. Staff attestation records. '
    'presence_threshold_id proves which threshold qualified this user for attestation. '
    'reviewers_distinct constraint prevents same person being both reviewer and staff. '
    'Two reviewers must co-sign within 48 hours — enforced at application layer. '
    'Rejection returns user to presence_confirmed status. Full history in attestation_attempts.';

-- Deferred FKs to visit_attestations
ALTER TABLE soultokens
    ADD CONSTRAINT soultokens_attestation_fk
    FOREIGN KEY (attestation_id) REFERENCES visit_attestations(id);

-- BFIP Section 6.8 | Full attestation attempt history including rejections
CREATE TABLE attestation_attempts (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    attestation_id                  INTEGER NOT NULL
                                    REFERENCES visit_attestations(id),
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    assigned_reviewer_1_id          INTEGER NOT NULL REFERENCES users(id),
    assigned_reviewer_2_id          INTEGER NOT NULL REFERENCES users(id),
    attempt_number                  INTEGER NOT NULL,
    outcome                         TEXT NOT NULL
                                    CHECK (outcome IN (
                                        'approved',
                                        'rejected',
                                        'expired'
                                        -- expired: co_sign_deadline passed without both signatures
                                    )),
    rejection_reason                TEXT,
    rejection_reviewer_id           INTEGER REFERENCES users(id),
    occurred_at                     TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE attestation_attempts IS
    'BFIP Section 6.8. Full attestation attempt history — append-only. '
    'Every attempt including rejections and expirations is preserved permanently. '
    'Provides complete audit trail of how many times a user attempted attestation '
    'and why each attempt failed.';

-- =============================================================
-- SECTION 7: BOXES AND ORDERS
-- =============================================================

-- BFIP Section 9 | Box NFC chip chain of custody
CREATE TABLE visit_boxes (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    assigned_order_id               INTEGER,
                                    -- FK added after orders created
                                    -- null for first-come-first-served boxes
    nfc_chip_uid                    TEXT NOT NULL UNIQUE,
                                    -- hardware chip identifier
    quantity                        INTEGER NOT NULL DEFAULT 1,
                                    -- number of strawberries in this box
    chain_of_custody_hash           TEXT,
                                    -- SHA-256 of pack_signature || delivery_signature
                                    -- nullable in v0.1.0, required in future version
    pack_signature                  TEXT,
                                    -- Box Fraise facility Secure Enclave signature at pack time
    delivery_signature              TEXT,
                                    -- delivery staff Secure Enclave signature at activation
    activated_at                    TIMESTAMPTZ,
                                    -- when delivery staff activated the chip
    expires_at                      TIMESTAMPTZ,
                                    -- delivery window end — chip invalid after this
    tapped_by_user_id               INTEGER REFERENCES users(id),
    tapped_at                       TIMESTAMPTZ,
                                    -- single-use: tapped_at set on first tap
    collection_confirmed_at         TIMESTAMPTZ,
                                    -- when box was physically handed over
                                    -- distinct from tapped_at
    clone_detected                  BOOLEAN NOT NULL DEFAULT false,
                                    -- true if tap attempted after tapped_at already set
    clone_detected_at               TIMESTAMPTZ,
    clone_alert_sent_at             TIMESTAMPTZ,
                                    -- when platform admin alert was sent
    returned_at                     TIMESTAMPTZ,
                                    -- if box was returned unused after delivery window
    disposal_reason                 TEXT
                                    CHECK (disposal_reason IN (
                                        'returned',
                                        'disposed',
                                        'unknown'
                                    )),
    is_gift                         BOOLEAN NOT NULL DEFAULT false,
    gift_reason                     TEXT
                                    CHECK (gift_reason IN (
                                        'support_interaction',
                                        'welcome',
                                        'platform_coverage',
                                        'referral'
                                    )),
    covered_by                      TEXT
                                    CHECK (covered_by IN (
                                        'user',
                                        'platform'
                                    )),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE visit_boxes IS
    'BFIP Section 9. Box NFC chip chain of custody records. '
    'Single-use: tapped_at set on first tap. Second tap sets clone_detected = true. '
    'chain_of_custody_hash covers pack_signature || delivery_signature — nullable in v0.1.0. '
    'collection_confirmed_at is distinct from tapped_at — tap proves presence, '
    'collection_confirmed_at proves physical handover.';

-- Deferred FK from presence_events to visit_boxes
ALTER TABLE presence_events
    ADD CONSTRAINT presence_events_box_fk
    FOREIGN KEY (box_id) REFERENCES visit_boxes(id);

-- Deferred FKs from soultokens
ALTER TABLE soultokens
    ADD CONSTRAINT soultokens_identity_credential_fk
    FOREIGN KEY (identity_credential_id) REFERENCES identity_credentials(id);

-- Strawberry purchase orders
CREATE TABLE orders (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    business_id                     INTEGER NOT NULL REFERENCES businesses(id),
    visit_id                        INTEGER REFERENCES staff_visits(id),
    stripe_payment_intent_id        TEXT UNIQUE,
    variety_description             TEXT,
                                    -- what was ordered (catalog removed but product concept remains)
    box_count                       INTEGER NOT NULL DEFAULT 1,
                                    -- number of boxes ordered
    amount_cents                    INTEGER NOT NULL,
    status                          TEXT NOT NULL DEFAULT 'pending'
                                    CHECK (status IN (
                                        'pending',
                                        'paid',
                                        'collected',
                                        'cancelled'
                                    )),
    collected_via_box_id            INTEGER REFERENCES visit_boxes(id),
    pickup_deadline                 TIMESTAMPTZ,
                                    -- food safety — must collect before this time
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE orders IS
    'Strawberry purchase orders. Tied to a staff visit for collection. '
    'pickup_deadline enforces food safety — uncollected orders flagged after deadline. '
    'Connection to presence events via: collected_via_box_id → visit_boxes → presence_events.';

-- Deferred FK from visit_boxes to orders
ALTER TABLE visit_boxes
    ADD CONSTRAINT visit_boxes_order_fk
    FOREIGN KEY (assigned_order_id) REFERENCES orders(id);

-- =============================================================
-- SECTION 8: SUPPORT
-- =============================================================

-- BFIP Section 10 | User support booking records
CREATE TABLE support_bookings (
    id                              SERIAL PRIMARY KEY,
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    issue_description               TEXT,
                                    -- minimal free text, kept private
    priority                        TEXT NOT NULL DEFAULT 'standard'
                                    CHECK (priority IN (
                                        'standard',
                                        'urgent'
                                    )),
    status                          TEXT NOT NULL DEFAULT 'booked'
                                    CHECK (status IN (
                                        'booked',
                                        'attended',
                                        'resolved',
                                        'no_show',
                                        'cancelled'
                                    )),
    booking_confirmation_sent_at    TIMESTAMPTZ,
    reminder_sent_at                TIMESTAMPTZ,
    attended_at                     TIMESTAMPTZ,
                                    -- when user arrived for support interaction
    cancelled_at                    TIMESTAMPTZ,
    cancellation_reason             TEXT,
    rescheduled_to_visit_id         INTEGER REFERENCES staff_visits(id),
                                    -- if rescheduled after no_show
    resolved_at                     TIMESTAMPTZ,
    resolution_description          TEXT,
    resolution_staff_id             INTEGER REFERENCES users(id),
                                    -- delivery staff who resolved in person
    resolution_signature            TEXT,
                                    -- Secure Enclave signature from resolving staff
    gift_box_provided               BOOLEAN NOT NULL DEFAULT false,
    attestation_id                  INTEGER REFERENCES visit_attestations(id),
                                    -- if attestation completed during this support booking
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE support_bookings IS
    'BFIP Section 10. User support booking records. '
    'Support interactions happen during staff visits — same unified visit model. '
    'resolution_signature is Secure Enclave signed by the resolving delivery staff member. '
    'attestation_id links to attestation if user completed Stage 4 during this support visit.';

-- One active booking per user per visit (prevent overbooking)
CREATE UNIQUE INDEX idx_support_bookings_one_active_per_visit
    ON support_bookings(visit_id, user_id)
    WHERE status NOT IN ('cancelled', 'no_show');

-- BFIP Section 17.4 | Platform-covered gift box audit trail
CREATE TABLE gift_box_history (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    visit_id                        INTEGER NOT NULL REFERENCES staff_visits(id),
    box_id                          INTEGER REFERENCES visit_boxes(id),
    gift_reason                     TEXT NOT NULL
                                    CHECK (gift_reason IN (
                                        'support_interaction',
                                        'welcome',
                                        'platform_coverage',
                                        'referral'
                                    )),
    covered_by                      TEXT NOT NULL
                                    CHECK (covered_by IN (
                                        'user',
                                        'platform'
                                    )),
    gifted_at                       TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE gift_box_history IS
    'Append-only platform-covered gift box audit trail. '
    'Enforces one platform-covered gift per user per 6 months via users.platform_gift_eligible_after. '
    'Full history preserved for dispute resolution.';

-- =============================================================
-- SECTION 9: AUDIT AND TOKENS
-- =============================================================

-- BFIP Section 14, Appendix A | User verification journey audit trail
CREATE TABLE verification_events (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    event_type                      TEXT NOT NULL
                                    CHECK (event_type IN (
                                        'identity_confirmed',
                                        'background_check_initiated',
                                        'background_check_passed',
                                        'background_check_failed',
                                        'background_check_review_required',
                                        'cleared_status_granted',
                                        'cleared_status_revoked',
                                        'cooling_period_started',
                                        'cooling_app_open_recorded',
                                        'cooling_period_completed',
                                        'cooling_period_failed',
                                        'presence_event_recorded',
                                        'presence_session_completed',
                                        'presence_reset',
                                        'presence_threshold_met',
                                        'attestation_initiated',
                                        'attestation_co_sign_pending',
                                        'attestation_approved',
                                        'attestation_rejected',
                                        'attestation_expired',
                                        'soultoken_issued',
                                        'soultoken_renewed',
                                        'soultoken_revoked',
                                        'soultoken_surrender_requested',
                                        'soultoken_surrender_completed',
                                        'business_approaching_suspension',
                                        'business_suspended',
                                        'beacon_suspended',
                                        'reviewer_missed_deadline',
                                        'reviewer_reassigned',
                                        'status_changed'
                                    )),
    from_status                     TEXT,
    to_status                       TEXT,
    reference_id                    INTEGER,
    reference_type                  TEXT
                                    CHECK (reference_type IN (
                                        'staff_visit',
                                        'visit_attestation',
                                        'presence_event',
                                        'presence_session',
                                        'identity_credential',
                                        'presence_threshold',
                                        'soultoken',
                                        'business',
                                        'beacon'
                                    )),
    actor_id                        INTEGER REFERENCES users(id),
    metadata                        JSONB NOT NULL DEFAULT '{}',
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE verification_events IS
    'BFIP Section 14, Appendix A. Append-only user verification journey audit trail. '
    'Every status transition and protocol event is recorded here. '
    'reference_type CHECK constraint prevents silent typos in join queries. '
    'Used for user audit trail delivery (BFIP Section 17).';

-- BFIP Section 11 | Third-party attestation tokens
CREATE TABLE attestation_tokens (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    soultoken_id                    INTEGER NOT NULL REFERENCES soultokens(id),
    scope                           TEXT NOT NULL DEFAULT 'presence.verified'
                                    CHECK (scope IN (
                                        'presence.verified',
                                        'presence.location',
                                        'identity.age'
                                    )),
    scope_metadata                  JSONB,
                                    -- additional scope-specific claims
                                    -- future scopes may require metadata
    token_hash                      TEXT NOT NULL UNIQUE,
                                    -- SHA-256 of raw token — never store plaintext
                                    -- raw token returned to user once only
    requesting_business_soultoken_id INTEGER REFERENCES soultokens(id),
                                    -- third party must be a verified business
    user_device_id                  TEXT,
                                    -- device that initiated the token presentation
    presentation_latitude           NUMERIC(9,6),
    presentation_longitude          NUMERIC(9,6),
    presentation_device_signature   TEXT,
                                    -- device Secure Enclave signature proving device presented token
    user_presented                  BOOLEAN NOT NULL DEFAULT false,
                                    -- true when user initiates presentation
                                    -- prevents forwarded/intercepted token use
    issued_at                       TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at                      TIMESTAMPTZ NOT NULL
                                    DEFAULT now() + INTERVAL '15 minutes',
    verified_at                     TIMESTAMPTZ,
    revoked_at                      TIMESTAMPTZ
);

COMMENT ON TABLE attestation_tokens IS
    'BFIP Section 11. Short-lived scoped tokens for third-party verification. '
    'User-initiated only — third parties cannot query arbitrary tokens. '
    'token_hash is SHA-256 of raw token — raw token returned to user once and never stored. '
    'requesting_business_soultoken_id enforces third party must be verified business. '
    'presentation_device_signature proves token was presented by the issuing device.';

-- BFIP Section 11.5 | Third-party verification attempt log
CREATE TABLE third_party_verification_attempts (
    id                              SERIAL PRIMARY KEY,
    token_hash                      TEXT NOT NULL,
    attestation_token_id            INTEGER REFERENCES attestation_tokens(id),
                                    -- null when outcome = 'not_found'
    requesting_business_soultoken_id INTEGER REFERENCES soultokens(id),
    request_signature               TEXT,
                                    -- business signs request proving they hold soultoken key
    ip_address                      TEXT,
    user_agent                      TEXT,
    outcome                         TEXT NOT NULL
                                    CHECK (outcome IN (
                                        'success',
                                        'not_found',
                                        'expired',
                                        'already_verified',
                                        'revoked'
                                    )),
    attempted_at                    TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE third_party_verification_attempts IS
    'BFIP Section 11.5. Third-party token verification attempt log. '
    'Rate limiting and abuse detection via ip_address and requesting_business_soultoken_id. '
    'request_signature proves business holds private key matching their soultoken. '
    'attestation_token_id null when outcome = not_found.';

-- BFIP Section 14 | Global immutable platform audit log
CREATE TABLE audit_events (
    id                              SERIAL PRIMARY KEY,
    event_kind                      TEXT NOT NULL,
    user_id                         INTEGER REFERENCES users(id),
    actor_id                        INTEGER REFERENCES users(id),
    metadata                        JSONB NOT NULL DEFAULT '{}',
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE audit_events IS
    'BFIP Section 14. Global append-only platform audit log. '
    'Written by every domain for security-relevant events. '
    'Immutable — protected by bf_prevent_modification trigger.';

-- BFIP Section 17.4 | User audit trail request log
CREATE TABLE audit_request_log (
    id                              SERIAL PRIMARY KEY,
    user_id                         INTEGER NOT NULL REFERENCES users(id),
    requested_by                    INTEGER NOT NULL REFERENCES users(id),
                                    -- usually same as user_id, but platform_admin can request
    delivery_method                 TEXT NOT NULL
                                    CHECK (delivery_method IN (
                                        'in_app',
                                        'in_person'
                                    )),
    requested_at                    TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE audit_request_log IS
    'BFIP Section 17.4. Append-only record of user audit trail requests. '
    'Proves the platform honoured the user''s right of access (PIPEDA, GDPR Article 15). '
    'requested_by is usually the user themselves but platform_admin can request on behalf.';

-- Deferred FK from users to cleared soultokens
ALTER TABLE users
    ADD CONSTRAINT users_cleared_soultoken_fk
    FOREIGN KEY (cleared_soultoken_id) REFERENCES soultokens(id);

-- Deferred FK for attested_soultoken_id on cleared soultokens
ALTER TABLE soultokens
    ADD CONSTRAINT soultokens_attested_soultoken_fk
    FOREIGN KEY (attested_soultoken_id) REFERENCES soultokens(id);

-- =============================================================
-- RESERVED: MESH NETWORK TABLES (extensions/mesh.md)
-- These tables are not created in v0.1.2.
-- They are documented here as reserved for the mesh extension.
--
-- beacon_peers — neighboring beacon relationships
--   beacon_id, peer_beacon_id, discovered_at, last_seen_at
--
-- mesh_events — inter-beacon communications
--   source_beacon_id, destination_beacon_id, event_type,
--   payload_hash, relayed_at
--
-- offline_presence_events — presence events relayed through mesh
--   presence_event_id, relay_beacon_id, relay_device_id,
--   relayed_at, server_received_at
--
-- verified_encounters — two attested users in physical proximity
--   user_a_id, user_b_id, location_beacon_id, proximity_cm,
--   user_a_signature, user_b_signature, occurred_at
-- =============================================================



-- BFIP Section 15 | Runtime-configurable protocol parameters
CREATE TABLE platform_configuration (
    id                              SERIAL PRIMARY KEY,
    key                             TEXT NOT NULL UNIQUE,
    value                           TEXT NOT NULL,
    value_type                      TEXT NOT NULL
                                    CHECK (value_type IN (
                                        'integer',
                                        'interval',
                                        'boolean',
                                        'text'
                                    )),
    description                     TEXT NOT NULL,
    cache_ttl_seconds               INTEGER NOT NULL DEFAULT 300,
                                    -- how long this value can be cached before re-read
    updated_by                      INTEGER REFERENCES users(id),
    updated_at                      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE platform_configuration IS
    'BFIP Section 15. Runtime-configurable protocol parameters. '
    'Changes do not require code deployment. '
    'Every change is recorded in platform_configuration_history. '
    'cache_ttl_seconds controls how long values can be cached by application layer.';

-- BFIP Section 15 | Configuration change history
CREATE TABLE platform_configuration_history (
    id                              SERIAL PRIMARY KEY,
    configuration_id                INTEGER NOT NULL
                                    REFERENCES platform_configuration(id),
    previous_value                  TEXT NOT NULL,
    new_value                       TEXT NOT NULL,
    changed_by                      INTEGER NOT NULL REFERENCES users(id),
    changed_at                      TIMESTAMPTZ NOT NULL DEFAULT now()
);

COMMENT ON TABLE platform_configuration_history IS
    'BFIP Section 15. Append-only configuration change history. '
    'Previous values preserved — no configuration change is ever lost. '
    'Immutable — protected by bf_prevent_modification trigger.';

-- =============================================================
-- SECTION 11: INDEXES
-- =============================================================

-- Users
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_apple_id ON users(apple_id);
CREATE INDEX idx_users_verification_status ON users(verification_status);
CREATE INDEX idx_users_soultoken_id ON users(soultoken_id);
CREATE INDEX idx_users_active ON users(id) WHERE deleted_at IS NULL;

-- Auth
CREATE INDEX idx_magic_link_tokens_token_hash ON magic_link_tokens(token_hash);
CREATE INDEX idx_magic_link_tokens_user_id ON magic_link_tokens(user_id);
CREATE INDEX idx_jwt_revocations_jti ON jwt_revocations(jti);
CREATE INDEX idx_jwt_revocations_expires_at ON jwt_revocations(expires_at);
CREATE INDEX idx_apple_auth_sessions_user_id ON apple_auth_sessions(user_id);

-- Background checks
CREATE INDEX idx_background_checks_user_id ON background_checks(user_id);
CREATE INDEX idx_background_checks_status ON background_checks(user_id, status);

-- Identity credentials
CREATE INDEX idx_identity_credentials_user_id ON identity_credentials(user_id);
CREATE INDEX idx_cooling_period_events_user_credential_date
    ON cooling_period_events(user_id, credential_id, calendar_date);

-- Soultokens
CREATE INDEX idx_soultokens_uuid ON soultokens(uuid);
CREATE INDEX idx_soultokens_display_code ON soultokens(display_code);
CREATE INDEX idx_soultokens_holder_user_id ON soultokens(holder_user_id);
CREATE INDEX idx_soultokens_expires_at ON soultokens(expires_at);
CREATE INDEX idx_soultokens_business_id ON soultokens(business_id);

-- Locations and businesses
CREATE INDEX idx_locations_location_type ON locations(location_type);
CREATE INDEX idx_businesses_location_id ON businesses(location_id);
CREATE INDEX idx_businesses_primary_holder_id ON businesses(primary_holder_id);
CREATE INDEX idx_businesses_verification_status ON businesses(verification_status);
CREATE INDEX idx_businesses_active ON businesses(id) WHERE deleted_at IS NULL;

-- Beacons
CREATE INDEX idx_beacons_location_id ON beacons(location_id);
CREATE INDEX idx_beacons_business_id ON beacons(business_id);
CREATE INDEX idx_beacon_rotation_log_beacon_date
    ON beacon_rotation_log(beacon_id, calendar_date);
CREATE INDEX idx_beacon_health_log_beacon_checked
    ON beacon_health_log(beacon_id, checked_at);

-- Staff
CREATE INDEX idx_staff_roles_user_id ON staff_roles(user_id);
CREATE INDEX idx_staff_roles_location_id ON staff_roles(location_id);
CREATE INDEX idx_staff_roles_active
    ON staff_roles(user_id, role)
    WHERE revoked_at IS NULL;
CREATE INDEX idx_reviewer_assignment_log_visit_id
    ON reviewer_assignment_log(visit_id);
CREATE INDEX idx_reviewer_assignment_log_reviewer_id
    ON reviewer_assignment_log(reviewer_id);

-- Staff visits
CREATE INDEX idx_staff_visits_location_id ON staff_visits(location_id);
CREATE INDEX idx_staff_visits_staff_id ON staff_visits(staff_id);
CREATE INDEX idx_staff_visits_scheduled_at ON staff_visits(scheduled_at);
CREATE INDEX idx_staff_visits_status ON staff_visits(status);
CREATE INDEX idx_visit_signatures_visit_id ON visit_signatures(visit_id);
CREATE INDEX idx_visit_signatures_reviewer_id ON visit_signatures(reviewer_id);
CREATE INDEX idx_quality_assessments_business_id ON quality_assessments(business_id);
CREATE INDEX idx_business_assessment_history_rolling
    ON business_assessment_history(business_id, assessed_at);

-- Presence
CREATE INDEX idx_presence_sessions_user_id ON presence_sessions(user_id);
CREATE INDEX idx_presence_sessions_business_id ON presence_sessions(business_id);
CREATE INDEX idx_presence_events_user_id ON presence_events(user_id);
CREATE INDEX idx_presence_events_business_date
    ON presence_events(user_id, business_id, calendar_date);
CREATE INDEX idx_presence_events_qualifying
    ON presence_events(user_id, is_qualifying);
CREATE INDEX idx_presence_thresholds_user_id ON presence_thresholds(user_id);
CREATE INDEX idx_presence_thresholds_business_id ON presence_thresholds(business_id);
CREATE INDEX idx_qualifying_presence_events_threshold
    ON qualifying_presence_events(threshold_id);

-- Attestations
CREATE INDEX idx_visit_attestations_user_id ON visit_attestations(user_id);
CREATE INDEX idx_visit_attestations_status ON visit_attestations(status);
CREATE INDEX idx_visit_attestations_visit_id ON visit_attestations(visit_id);
CREATE INDEX idx_attestation_attempts_user_id ON attestation_attempts(user_id);
CREATE INDEX idx_attestation_attempts_attestation_id
    ON attestation_attempts(attestation_id);

-- Boxes and orders
CREATE INDEX idx_visit_boxes_visit_id ON visit_boxes(visit_id);
CREATE INDEX idx_visit_boxes_nfc_chip_uid ON visit_boxes(nfc_chip_uid);
CREATE INDEX idx_visit_boxes_tapped_by ON visit_boxes(tapped_by_user_id);
CREATE INDEX idx_orders_user_id ON orders(user_id);
CREATE INDEX idx_orders_business_id ON orders(business_id);
CREATE INDEX idx_orders_stripe_pi ON orders(stripe_payment_intent_id);

-- Support
CREATE INDEX idx_support_bookings_visit_id ON support_bookings(visit_id);
CREATE INDEX idx_support_bookings_user_id ON support_bookings(user_id);
CREATE INDEX idx_gift_box_history_user_gifted
    ON gift_box_history(user_id, gifted_at);

-- Soultoken renewals
CREATE INDEX idx_soultoken_renewals_soultoken_id
    ON soultoken_renewals(soultoken_id);
CREATE INDEX idx_soultoken_renewals_user_id ON soultoken_renewals(user_id);

-- Audit and tokens
CREATE INDEX idx_verification_events_user_id ON verification_events(user_id);
CREATE INDEX idx_verification_events_event_type ON verification_events(event_type);
CREATE INDEX idx_attestation_tokens_token_hash ON attestation_tokens(token_hash);
CREATE INDEX idx_attestation_tokens_expires_at ON attestation_tokens(expires_at);
CREATE INDEX idx_third_party_attempts_token_hash
    ON third_party_verification_attempts(token_hash, attempted_at);
CREATE INDEX idx_audit_events_user_id ON audit_events(user_id);
CREATE INDEX idx_audit_events_event_kind ON audit_events(event_kind);
CREATE INDEX idx_audit_events_created_at ON audit_events(created_at);
CREATE INDEX idx_audit_request_log_user_id ON audit_request_log(user_id);

-- Platform configuration
CREATE INDEX idx_platform_configuration_key ON platform_configuration(key);
CREATE INDEX idx_platform_configuration_history_config
    ON platform_configuration_history(configuration_id, changed_at);

-- =============================================================
-- SECTION 12: TRIGGERS
-- =============================================================

-- Append-only protection
CREATE OR REPLACE FUNCTION bf_prevent_modification()
RETURNS TRIGGER AS $$
BEGIN
    RAISE EXCEPTION '% is append-only — insert new records instead of modifying existing ones',
        TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_events_immutable
    BEFORE UPDATE OR DELETE ON audit_events
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

CREATE TRIGGER verification_events_immutable
    BEFORE UPDATE OR DELETE ON verification_events
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

CREATE TRIGGER attestation_attempts_immutable
    BEFORE UPDATE OR DELETE ON attestation_attempts
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

CREATE TRIGGER gift_box_history_immutable
    BEFORE UPDATE OR DELETE ON gift_box_history
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

CREATE TRIGGER business_assessment_history_immutable
    BEFORE UPDATE OR DELETE ON business_assessment_history
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

CREATE TRIGGER platform_configuration_history_immutable
    BEFORE UPDATE OR DELETE ON platform_configuration_history
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

CREATE TRIGGER audit_request_log_immutable
    BEFORE UPDATE OR DELETE ON audit_request_log
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_modification();

-- Prevent renewal of revoked soultokens
CREATE OR REPLACE FUNCTION bf_prevent_revoked_soultoken_renewal()
RETURNS TRIGGER AS $$
BEGIN
    IF (SELECT revoked_at FROM soultokens WHERE id = NEW.soultoken_id) IS NOT NULL THEN
        RAISE EXCEPTION 'Cannot renew a revoked soultoken';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER soultoken_renewal_not_revoked
    BEFORE INSERT ON soultoken_renewals
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_revoked_soultoken_renewal();

-- Prevent double attestation of already-attested users
CREATE OR REPLACE FUNCTION bf_prevent_double_attestation()
RETURNS TRIGGER AS $$
BEGIN
    IF (SELECT verification_status FROM users WHERE id = NEW.user_id) = 'attested' THEN
        RAISE EXCEPTION 'User % is already attested', NEW.user_id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER attestation_not_already_attested
    BEFORE INSERT ON visit_attestations
    FOR EACH ROW EXECUTE FUNCTION bf_prevent_double_attestation();

-- Cascade beacon suspension when business is suspended
CREATE OR REPLACE FUNCTION bf_cascade_beacon_suspension()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.beacon_suspended = true AND OLD.beacon_suspended = false THEN
        UPDATE beacons
        SET is_active = false,
            updated_at = now()
        WHERE business_id = NEW.id;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER business_beacon_suspension_cascade
    AFTER UPDATE ON businesses
    FOR EACH ROW EXECUTE FUNCTION bf_cascade_beacon_suspension();

-- Auto-update updated_at on modification
CREATE OR REPLACE FUNCTION bf_update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER businesses_updated_at
    BEFORE UPDATE ON businesses
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER beacons_updated_at
    BEFORE UPDATE ON beacons
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER staff_visits_updated_at
    BEFORE UPDATE ON staff_visits
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER visit_attestations_updated_at
    BEFORE UPDATE ON visit_attestations
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER support_bookings_updated_at
    BEFORE UPDATE ON support_bookings
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER orders_updated_at
    BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

CREATE TRIGGER presence_thresholds_updated_at
    BEFORE UPDATE ON presence_thresholds
    FOR EACH ROW EXECUTE FUNCTION bf_update_updated_at();

-- =============================================================
-- SECTION 13: SEED DATA REQUIRED
-- =============================================================
--
-- The following records must be inserted manually before the
-- platform can operate. Run after initial deployment.
--
-- 1. First platform_admin user
--    INSERT INTO users (email, is_platform_admin, verification_status)
--    VALUES ('admin@boxfraise.com', true, 'attested');
--
-- 2. First box_fraise_store location
--    INSERT INTO locations (name, location_type, address, timezone)
--    VALUES ('Box Fraise Edmonton', 'box_fraise_store',
--            '...', 'America/Edmonton');
--
-- 3. Platform configuration initial values
--    INSERT INTO platform_configuration (key, value, value_type, description) VALUES
--    ('cooling_period_days', '7', 'integer',
--     'Days from identity confirmation before cooling period ends'),
--    ('cooling_app_opens_required', '3', 'integer',
--     'App opens required on separate days during cooling period'),
--    ('presence_events_required', '3', 'integer',
--     'Qualifying presence events required for Stage 3'),
--    ('presence_days_required', '3', 'integer',
--     'Separate calendar days required for Stage 3'),
--    ('min_dwell_minutes', '15', 'integer',
--     'Minimum beacon dwell time in minutes for a qualifying event'),
--    ('default_rssi_threshold', '-70', 'integer',
--     'Default minimum RSSI in dBm for beacon presence events'),
--    ('soultoken_expiry_months', '12', 'integer',
--     'Soultoken validity period in months'),
--    ('attestation_token_expiry_minutes', '15', 'integer',
--     'Third-party attestation token validity in minutes'),
--    ('co_sign_deadline_hours', '48', 'integer',
--     'Hours reviewers have to co-sign an attestation'),
--    ('platform_gift_limit_months', '6', 'integer',
--     'Months between platform-covered gift boxes per user'),
--    ('delivery_staff_reveal_hours', '2', 'integer',
--     'Hours before window to reveal exact schedule to delivery staff'),
--    ('background_check_expiry_months', '12', 'integer',
--     'Months before a background check result expires and must be re-run'),
--    ('cleared_requires_all_checks', 'true', 'boolean',
--     'All five background check types required for cleared status'),
--
-- =============================================================
-- TEST FIXTURE MINIMUM
-- =============================================================
--
-- Minimum records to create one attested user end to end:
--
-- 1. INSERT INTO users (email) VALUES ('test@example.com')
-- 2. INSERT INTO identity_credentials (user_id, credential_type, verified_at,
--    cooling_ends_at) VALUES (1, 'stripe_identity', now(), now() + 7 days)
-- 3. INSERT INTO cooling_period_events x3 on separate days
-- 4. INSERT INTO locations (box_fraise_store)
-- 5. INSERT INTO businesses (location_id, primary_holder_id)
-- 6. INSERT INTO beacons (location_id, business_id, secret_key)
-- 7. INSERT INTO presence_sessions x3
-- 8. INSERT INTO presence_events x3 (is_qualifying = true)
-- 9. INSERT INTO presence_thresholds (user_id, business_id, event_count=3, days_count=3)
-- 10. INSERT INTO qualifying_presence_events x3
-- 11. INSERT INTO staff_visits
-- 12. INSERT INTO visit_attestations
-- 13. INSERT INTO visit_signatures x2
-- 14. INSERT INTO soultokens
-- 15. UPDATE users SET soultoken_id = ..., verification_status = 'attested'
-- =============================================================
