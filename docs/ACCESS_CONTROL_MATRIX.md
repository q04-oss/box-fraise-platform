# Access Control Matrix

**Hardening Section 2a — design document, no enforcement code yet.**

This file inventories every principal in box-fraise-platform, every database
table, and the access rules that today live in service-layer Rust. It is the
authoritative reference for a future migration that lifts those rules into
PostgreSQL row-level security policies and dedicated database roles.

> Schema reference: `server/migrations/001_bfip_schema.sql`. Spec said "37
> tables"; the live schema actually has **38** (the count below includes
> `qualifying_presence_events`, which the spec count likely missed).

## Section 1 — Principals

The application authenticates every request to one of these principals before
service code runs. "Principal" means *the role the request is acting as* —
some users hold several roles simultaneously and the principal escalates to
the highest one the request requires.

| Principal | Source of identity | Notes |
|-----------|-------------------|-------|
| `anonymous` | No JWT, no API key, no webhook signature | Reaches only the public endpoints in Section 4 of the table matrix. |
| `user` | Valid user JWT (`RequireUser`) | Any `verification_status`. Default principal for all `/api/*` routes that need an identity. |
| `identity_confirmed_user` | `user` + `users.verification_status = 'identity_confirmed'` | Cooling period running or complete. Required to start cooling events. |
| `presence_confirmed_user` | `user` + `users.verification_status = 'presence_confirmed'` | Threshold met; eligible to be attested. |
| `attested_user` | `user` + `users.verification_status = 'attested'` + non-revoked `users.soultoken_id` | Required to create businesses and beacons. |
| `cleared_user` | `attested_user` + non-revoked `users.cleared_soultoken_id` | Optional elevated tier; reserved for future scopes. |
| `delivery_staff` | `user` + active `staff_roles` row with `role='delivery_staff'` | Location-scoped via `staff_roles.location_id` (NOT NULL for this role). |
| `attestation_reviewer` | `user` + active `staff_roles` row with `role='attestation_reviewer'` | Platform-wide; assigned per-attestation by `assign_reviewers_for_visit`. |
| `platform_admin` | `users.is_platform_admin = true` | **Sole** enforcement path. Migration 008 deleted any historical `staff_roles` rows with `role='platform_admin'` and added a CHECK constraint preventing re-introduction. `grant_staff_role` rejects the role with `DomainError::InvalidInput`. Anchor doc-comment lives on `domain::domain::auth::types::UserRow::is_platform_admin`. |
| `stripe_webhook` | Stripe signature header verified by `integrations::stripe::verify_signature` | No JWT. May only act on identity / payment endpoints. |
| `background_check_webhook` | Provider-specific signature on the inbound webhook (currently a stub — provider integration pending) | No JWT. Restricted to `background_checks` writes. |
| `public` | No auth required by design | Trust registry, health check, OpenAPI doc. |

"Active" for any `staff_roles` row means: `revoked_at IS NULL AND (expires_at
IS NULL OR expires_at > now())`.

## Section 2 — Per-table access matrix

Conventions in the tables below:

- **owner** = the user the row is `about` (the `user_id` / `holder_user_id` /
  `tapped_by_user_id` column).
- **business owner** = `users.id = businesses.primary_holder_id` for the row's
  business.
- **system** = the application process under `app_user`, executing service
  code on behalf of the principal that triggered the call. No principal
  reaches these inserts directly via SQL.
- A blank cell means *no principal performs this operation today.*

### Auth tables (5)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `users` | owner; `platform_admin` (any); `public` for `/users/{id}/profile` (`display_name` only via the `users::service::get_public_profile` projection) | system on signup | owner (`display_name`, `push_token`); system (`verification_status`, `attested_at`, `last_active_at`); `platform_admin` (`is_banned`, `is_platform_admin`) | none — soft-delete via `deleted_at` |
| `apple_auth_sessions` | owner; `platform_admin` | system (Apple Sign-In flow) | system (`revoked_at`) | none |
| `magic_link_tokens` | system only (token never returned to client) | system (rate-limited per-email) | system (`used_at` on first verify) | system (purge after `expires_at`) |
| `jwt_revocations` | system | system (on `/auth/logout`) | none | system (daily prune of `expires_at < now()`) |
| `identity_credentials` | owner; `platform_admin` | system on Stripe Identity callback | system (`raw_verification_status`, `cooling_completed_at`) | none |

### Background check tables (1)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `background_checks` | owner; `platform_admin` | `background_check_webhook` (initial row) and system (initiate) | `background_check_webhook` (`status`, `checked_at`, `response_hash`) | none |
| `cooling_period_events` | owner; `platform_admin` | `identity_confirmed_user` (own user only; one per `calendar_date` enforced by UNIQUE) | none | none |

### Soultoken tables (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `soultokens` | holder (`get_my_soultoken`); `platform_admin`; `attestation_reviewer` (for revoke/surrender flow); `public` (display_code only via `attestation_tokens` verify) — `uuid` column never leaves the DB | system (issue + renew) | system (`signature` on renewal); `platform_admin` / `attestation_reviewer` via `revoke_soultoken`; holder via `surrender_soultoken` | none |
| `soultoken_renewals` | holder; `platform_admin` | system on renewal | none | none |

### Physical infrastructure (4)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `locations` | `user` (read public fields); `platform_admin` (full) | `platform_admin` | `platform_admin` | none |
| `businesses` | `user` (public profile via `get_public_profile`); business owner (full); `platform_admin` | `attested_user` (creates row with self as `primary_holder_id`) | business owner; `platform_admin` (`verification_status`, `beacon_suspended`, `suspended_at`) | none — soft-delete via `deleted_at` |
| `beacons` | business owner; `platform_admin`; **`secret_key` and `previous_secret_key` are never serialised** in `BeaconResponse` | business owner via `create_beacon` | business owner via `rotate_key`; system (`failure_count`, `last_seen_at`); `platform_admin` (`is_active`) | none |
| `beacon_rotation_log` | business owner; `platform_admin` | system (on every `get_daily_uuid` and `rotate_key`) | none | none |
| `beacon_health_log` | business owner; `platform_admin` | system (health check job) | none | none |

### Staff tables (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `staff_roles` | `user` (own active roles); `platform_admin` (any) | `platform_admin` (operational roles only — `delivery_staff`, `attestation_reviewer`; the `platform_admin` role string is rejected at the service layer and blocked by the CHECK constraint added in migration 008) | `platform_admin` (`revoked_at`, `expires_at`, `confirmed_by`, `confirmed_at`) | none |
| `reviewer_assignment_log` | `platform_admin`; assigned reviewer (own rows) | system on `initiate_attestation` | none | none |

### Staff visit tables (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `staff_visits` | assigned `delivery_staff`; business owner of `location_id`; `platform_admin`; users with rows referencing this visit (orders, support, attestation) | `delivery_staff` (own visits via `schedule_visit`); `platform_admin` | assigned `delivery_staff` (`arrived_at`, `arrived_latitude`, `arrived_longitude`, `departed_at`, `actual_box_count`, `delivery_signature`, `evidence_hash`); system (`business_notified_at`, `staff_revealed_at`, `cancelled_at`); `platform_admin` (any) | none |
| `staff_visit_notifications` | recipient (own); `platform_admin` | system (notification dispatcher) | system (`sent_at`, `delivered_at`, `read_at`) | none |

### Attestation tables (3)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `visit_signatures` | assigned reviewer (own row); attestation owner; `platform_admin`; auditor on `verify_aggregated_ed25519` re-check | system on `reviewer_sign` (after Ed25519 verify passes) | none | none |
| `visit_attestations` | attested user (own); assigned `delivery_staff`; assigned reviewers; `platform_admin` | `delivery_staff` via `initiate_attestation` | `delivery_staff` (`staff_signature`, `photo_hash`, `location_confirmed`, `user_present_confirmed`, `co_sign_deadline`); reviewers via `reviewer_sign` (status transition only); `platform_admin` (`status`) | none |
| `attestation_attempts` | attested user (own); assigned reviewers; `platform_admin` | system on every `approve_attestation` / `reject_attestation` / deadline expiry | **immutable** — `bf_prevent_modification` trigger | **immutable** |

### Verification protocol (4)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `presence_sessions` | owner; `platform_admin` | system (presence event aggregator) | system (`ended_at`, `total_dwell_minutes`, `is_qualifying`, `contributed_to_threshold_id`) | none |
| `presence_events` | owner; `platform_admin` | `user` (own user only, via `record_beacon_dwell` / `record_nfc_tap` after HMAC + RSSI checks) | system (`is_qualifying`, `rejection_reason`) | none |
| `presence_thresholds` | owner; `platform_admin` | system (first qualifying event) | system (`event_count`, `days_count`, `threshold_met_at`) | none |
| `qualifying_presence_events` | owner (via threshold join); `platform_admin` | system on threshold met | none | none |

### Quality (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `quality_assessments` | assessor; business owner; `platform_admin` | `delivery_staff` via `submit_quality_assessment` | none | none |
| `business_assessment_history` | business owner; `platform_admin` | system on `submit_quality_assessment` | **immutable** — trigger | **immutable** |

### Order tables (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `orders` | owner; assigned `delivery_staff`; `platform_admin` | `attested_user` via `create_order` | system (`status`, `collected_via_box_id`); owner via `cancel_order` (only while `status='pending'`); `delivery_staff` via `mark_collected` | none |
| `visit_boxes` | tapped user (own); assigned `delivery_staff`; `platform_admin` | `delivery_staff` (pack/load step) | `delivery_staff` (`activated_at`, `delivery_signature`); `user` via `tap_box` (`tapped_by_user_id`, `tapped_at`, `clone_detected`); system (`returned_at`, `disposal_reason`) | none |

### Support tables (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `support_bookings` | owner; assigned `delivery_staff`; `platform_admin` | `user` via `create_booking` | owner via `cancel_booking` (own, while `status='booked'`); `delivery_staff` via `attend_booking` and `resolve_booking`; system (`booking_confirmation_sent_at`, `reminder_sent_at`) | none |
| `gift_box_history` | owner; `platform_admin` | system on `resolve_booking` with platform gift | **immutable** — trigger | **immutable** |

### Audit and tokens (5)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `verification_events` | owner via `get_my_audit_trail`; `platform_admin` via `get_admin_audit_trail`; assigned reviewer (own actor rows) | system on every status transition | **immutable** — trigger | **immutable** |
| `attestation_tokens` | owner; `platform_admin`; raw token returned to owner once on issue; `requesting_business_soultoken_id` holder via `verify_token` | `attested_user` via `issue_token` | system (`verified_at`, `revoked_at`); owner via `revoke_token` | none |
| `third_party_verification_attempts` | `platform_admin`; requesting business soultoken holder (own rows only) | system on every `verify_token` call (including failures) | none | none |
| `audit_events` | `platform_admin` only | system (every domain) | **immutable** — trigger | **immutable** |
| `audit_request_log` | owner; `platform_admin` | system on every audit-trail request | **immutable** — trigger | **immutable** |

### Platform configuration (2)

| Table | SELECT | INSERT | UPDATE | DELETE |
|-------|--------|--------|--------|--------|
| `platform_configuration` | system (read at boot + on cache miss); `platform_admin` (read all) | system (default seed) and `platform_admin` (admin endpoint) | `platform_admin` (`value`, `cache_ttl_seconds`); system stamps `updated_by`, `updated_at` | none |
| `platform_configuration_history` | `platform_admin` | system on every `platform_configuration` update | **immutable** — trigger | **immutable** |

## Section 3 — RLS strategy

PostgreSQL row-level security is **not enabled today**. The matrix above is
enforced entirely by service-layer Rust running as the `app_user` role. The
plan below is what RLS would look like if enabled. The "key column" is the
column an RLS policy would compare against `current_setting('app.user_id')`.

| Table | RLS recommended | Policy type | Key column | Notes |
|-------|-----------------|-------------|------------|-------|
| `users` | yes | permissive | `id` | Plus admin-bypass policy. |
| `apple_auth_sessions` | yes | permissive | `user_id` | |
| `magic_link_tokens` | no | — | n/a | System-only writes; no client SELECT path. |
| `jwt_revocations` | no | — | n/a | System-only. |
| `identity_credentials` | yes | permissive | `user_id` | |
| `background_checks` | yes | permissive | `user_id` | Webhook writes via separate `app_admin` role. |
| `cooling_period_events` | yes | permissive | `user_id` | |
| `soultokens` | yes | permissive | `holder_user_id` | UUID column accessed only by service layer. |
| `soultoken_renewals` | yes | permissive | `user_id` | |
| `locations` | no | — | n/a | Public profile data. Restrict UPDATE/INSERT in roles. |
| `businesses` | yes | permissive | `primary_holder_id` | Plus a public-projection policy that exposes only `name`, `verification_status`. |
| `beacons` | yes | restrictive | `business_id` (joined to businesses) | RLS must additionally hide `secret_key` and `previous_secret_key`; safest answer is to revoke column-level SELECT on those two columns from `app_user`. |
| `beacon_rotation_log` | yes | permissive | `business_id` (joined) | |
| `beacon_health_log` | yes | permissive | `business_id` (joined) | |
| `staff_roles` | yes | permissive | `user_id` | Plus admin-bypass for `platform_admin`. |
| `reviewer_assignment_log` | yes | permissive | `reviewer_id` | Admin-bypass. |
| `staff_visits` | yes | permissive | `staff_id` OR location->business->holder | Multi-key RLS — likely simpler in service code than in policy. |
| `staff_visit_notifications` | yes | permissive | `recipient_id` | |
| `visit_signatures` | yes | permissive | `reviewer_id` | |
| `visit_attestations` | yes | permissive | `user_id` OR `staff_id` OR assigned reviewers | Same multi-key complexity as `staff_visits`. |
| `attestation_attempts` | yes | append-only restrictive | `user_id` | RLS must also enforce no UPDATE/DELETE — already covered by trigger. |
| `presence_sessions` | yes | permissive | `user_id` | |
| `presence_events` | yes | permissive | `user_id` | |
| `presence_thresholds` | yes | permissive | `user_id` | |
| `qualifying_presence_events` | yes | permissive | join via `threshold_id → user_id` | |
| `quality_assessments` | yes | permissive | `assessor_id` OR business holder | |
| `business_assessment_history` | yes | append-only restrictive | `business_id` (joined) | Trigger already prevents modification. |
| `orders` | yes | permissive | `user_id` | Plus assigned-staff bypass. |
| `visit_boxes` | yes | permissive | `tapped_by_user_id` (nullable) OR visit's `staff_id` | |
| `support_bookings` | yes | permissive | `user_id` | Plus assigned-staff bypass. |
| `gift_box_history` | yes | append-only restrictive | `user_id` | Trigger already prevents modification. |
| `verification_events` | yes | append-only restrictive | `user_id` | Trigger already prevents modification. |
| `attestation_tokens` | yes | permissive | `user_id` | Plus requesting-business-soultoken bypass for verify. |
| `third_party_verification_attempts` | yes | permissive | `requesting_business_soultoken_id` (joined to soultokens.holder_user_id) | Admin-bypass. |
| `audit_events` | yes | restrictive (admin only) | n/a | Only `app_admin` may SELECT. |
| `audit_request_log` | yes | permissive | `user_id` | |
| `platform_configuration` | no | — | n/a | Public read inside the app process; admin-only writes via role. |
| `platform_configuration_history` | yes | restrictive (admin only) | n/a | |

**Where RLS would conflict with existing triggers:** seven append-only tables
already have `bf_prevent_modification` triggers
(`audit_events`, `verification_events`, `attestation_attempts`,
`gift_box_history`, `business_assessment_history`,
`platform_configuration_history`, `audit_request_log`). Triggers fire after
RLS, so the two layers compose without conflict — RLS controls *who can read*
and *who can attempt to insert*, the trigger guarantees nothing modifies
existing rows. Don't try to encode the immutability rule in RLS — it's
correctly handled by the trigger.

## Section 4 — PostgreSQL roles

Today the application connects with one PostgreSQL superuser-equivalent role
(`fraise` per `docker-compose.yml`). The target is three roles with
least-privilege grants:

### `app_user` — application runtime

The role `box-fraise-server` connects as. Granted:

- `CONNECT` on the database.
- `USAGE` on `public` schema.
- `SELECT, INSERT, UPDATE` on every table EXCEPT the seven append-only ones,
  where it gets only `SELECT, INSERT`.
- `SELECT, USAGE` on every sequence (for SERIAL primary keys).
- **Revoked**: column-level `SELECT` on `beacons.secret_key`,
  `beacons.previous_secret_key`, `magic_link_tokens.token_hash`,
  `apple_auth_sessions.identity_token_hash`. The application has dedicated
  helpers that read those columns through the service layer, but no other
  query path needs them.
- **Revoked**: `DELETE` on every table. Soft-delete-only model.
- **Revoked**: any DDL.

### `app_readonly` — analytics / Grafana / Metabase

- `CONNECT` on the database.
- `USAGE` on `public` schema.
- `SELECT` on:
  - All tables EXCEPT `magic_link_tokens`, `apple_auth_sessions`,
    `jwt_revocations`, `attestation_tokens`,
    `third_party_verification_attempts`, `audit_events`,
    `audit_request_log`.
- **Revoked**: column-level `SELECT` on `beacons.secret_key`,
  `beacons.previous_secret_key`, `users.push_token`, `users.email`,
  `users.apple_id`, `soultokens.uuid`,
  `identity_credentials.external_session_id`,
  `identity_credentials.stripe_identity_report_id`,
  `identity_credentials.response_hash`,
  `background_checks.external_check_id`, `background_checks.response_hash`.
- **Revoked**: every write.

A future view layer (`analytics_*` views with PII columns hashed/dropped) is
the cleaner path; the column-level revokes above are the interim safe
default.

### `app_admin` — privileged platform operations

For data-correction tasks performed by on-call engineers and for the future
admin UI process. Granted:

- Everything `app_user` has.
- `SELECT` on the columns revoked from `app_user`.
- `SELECT` on `audit_events`, `audit_request_log`,
  `magic_link_tokens`, `jwt_revocations`,
  `attestation_tokens`, `third_party_verification_attempts`.
- **Revoked**: `DELETE` on every table (immutability is non-negotiable).
- **Revoked**: any DDL — schema migrations run as a fourth role
  (`app_migrations`) that is not used at runtime.

## Section 5 — Implementation notes

### Append-only tables (no UPDATE / no DELETE, ever)

`audit_events`, `verification_events`, `attestation_attempts`,
`gift_box_history`, `business_assessment_history`,
`platform_configuration_history`, `audit_request_log`. All seven are
protected by the `bf_prevent_modification` trigger
(schema lines 1645–1679). When migrating to roles, also revoke `UPDATE` and
`DELETE` at the grant level — defence in depth.

### Tables with non-immutability triggers

`soultokens` (`bf_prevent_revoked_soultoken_renewal`),
`visit_attestations` (`attestation_not_already_attested`),
`businesses` (`business_beacon_suspension_cascade`). RLS policies do not
interfere with these — design the policies as if the triggers weren't there
and let the triggers do their job.

### Admin-only tables

`audit_events` and `platform_configuration_history` should be readable only
by `platform_admin` principals at the application layer. `app_user` could
keep `SELECT` for now (the service layer gates it), but a future hardening
pass should split the `audit_events` writer into a dedicated stored
procedure callable by `app_user` while reads require `app_admin`.

### Fully public tables

None. The public surface area is built from projections in service code:

- `GET /api/users/{id}/public-profile` returns a hand-curated view of `users`
  (auth-gated — see Section 8 for the public surface inventory).
- `GET /api/businesses/{id}` returns a business view (auth-gated; there is
  **no** unauthenticated public projection of `businesses` today).
- `GET /api/trust-registry/public-key` returns no row data — only the
  configured Ed25519 verifying key.
- `GET /health` and `GET /api/docs/openapi.json` return no row data.

When the role split lands, keep these endpoints on `app_user` and rely on the
projection helpers to enforce the column allow-list.

### Tables where RLS would be misleading

- `magic_link_tokens` — the secret material is the token itself. Even owner
  reads are unsafe (the user already presented the token to redeem it; the
  service path consumes it without a SELECT round-trip to the client).
  Better: leave RLS off, lock SELECT to `app_user` only at the role grant
  level.
- `jwt_revocations` — pure system table. Same treatment.
- `platform_configuration` — read on the application boot path. Don't
  attempt user-scoped RLS; admin scoping happens at the service layer.

### Migration sequence (when this work happens)

1. Create `app_admin` role; rotate the existing `fraise` role into it (no
   privilege loss).
2. Create `app_user` and `app_readonly` with the grants above.
3. Switch the `box-fraise-server` `DATABASE_URL` to `app_user`.
4. Enable RLS on each table in the order listed in Section 3, validating the
   integration test suite stays green at every step.
5. Switch Grafana / Metabase connection strings to `app_readonly`.

This sequence is reversible at every step — drop the role and revoke the
RLS policies if a regression is found.

## Section 6 — Privileged and anonymous surfaces

Sections 1–5 catalogue *table* access. This section catalogues *route*
access for the two non-default principal classes the audit found missing
from the matrix: routes restricted to `platform_admin`, and routes that
accept no authentication at all (webhooks and public endpoints). Anything
not in this table is a normal authenticated user route.

### `platform_admin`-only routes (under `/api/admin/`)

Authorization at the service layer reads `users.is_platform_admin = true`
(see Section 1 row for `platform_admin` and the anchor doc-comment on
`domain::domain::auth::types::UserRow::is_platform_admin`).

| Method | Route | Purpose |
|--------|-------|---------|
| POST   | `/api/admin/users/{id}/ban` | Ban a user; revokes active soultokens (Hardening §10). |
| POST   | `/api/admin/users/{id}/unban` | Reverse a ban. |
| GET    | `/api/admin/audit/{user_id}` | Full `verification_events` audit trail for any user. |
| GET    | `/api/admin/analytics/funnel` | Verification-funnel metrics. |
| GET    | `/api/admin/analytics/attestations/daily` | Attestations per day. |
| GET    | `/api/admin/analytics/attestations/time-to-attest` | Median time from presence-confirmed → attested. |
| GET    | `/api/admin/analytics/businesses` | Per-business activity rollup. |
| GET    | `/api/admin/analytics/presence/daily` | Presence events per day. |
| GET    | `/api/admin/analytics/soultokens` | Soultoken issuance/revocation counts. |
| GET    | `/api/admin/analytics/background-checks` | Background-check pass/fail counts. |
| GET    | `/api/admin/analytics/conversion` | Funnel-to-conversion rate. |
| GET    | `/api/admin/configuration` | List every `platform_configuration` row. |
| GET    | `/api/admin/configuration/{key}` | Read one config key. |
| PATCH  | `/api/admin/configuration/{key}` | Update one config key (writes to `platform_configuration_history`). |
| GET    | `/api/admin/configuration/{key}/history` | Audit history for one key. |
| GET    | `/api/admin/feature-flags` | List feature flags. |
| PATCH  | `/api/admin/feature-flags/{flag_name}` | Enable/disable a feature flag globally. |
| GET    | `/api/admin/billing/subscriptions` | List business subscriptions (scaffolding — webhook ships post-iOS). |

### Anonymous routes (no JWT required)

These routes intentionally accept no auth header. Each has a dedicated
verification mechanism documented in the row.

| Method | Route | Verification | Notes |
|--------|-------|--------------|-------|
| POST   | `/api/identity/webhook/stripe` | `Stripe-Signature` header verified by `integrations::stripe::verify_signature` against `STRIPE_WEBHOOK_SECRET` | Restricted to identity / payment writes only. |
| POST   | `/api/background-checks/webhook` | Provider-specific HMAC (see `domain/src/domain/background_checks/service.rs::handle_webhook`); provider integration is currently a stub | Webhook signature key configured via `FRAISE_HMAC_SHARED_KEY`. |
| POST   | `/api/attestation-tokens/verify` | Token presented in body is single-use and signed; replay is structurally rejected by `verified_at`/`revoked_at` checks | Used by third-party verifiers; no caller identity is asserted. |
| GET    | `/api/trust-registry/public-key` | None — returns the configured Ed25519 verifying key for offline soultoken signature verification | Required for clients to verify soultokens without contacting the API. |
| GET    | `/health` | None — returns `{status, database, redis, storage, version}` | Used by UptimeRobot. Do not add row data. |
| GET    | `/api/docs/openapi.json` | None — returns the OpenAPI 3.1 document | Pair with `GET /api/docs` (Swagger UI). |
| GET    | `/metrics` | None at the application layer — **must be IP-restricted at nginx** (see `deploy/nginx.conf` loopback allow-list) | Prometheus scrape target. Exposes operational counters; not safe on the open internet. |
| GET    | `/.well-known/apple-app-site-association` | None — required by iOS Universal Links | Static JSON. |
| GET    | `/go?url=` | None — server-side allow-list of HTTPS destinations | Privacy-preserving redirect hop for transactional emails. |
| GET    | `/api/auth/magic-link/open` | None — deep-link redirect carrying the single-use magic-link token in the query string | Token is consumed by the subsequent `POST /api/auth/magic-link/verify`. |

### Discrepancies surfaced while writing this doc

1. **38 tables, not 37.** Spec said 37; live schema has 38
   (`qualifying_presence_events` is the off-by-one).
2. **`platform_admin` consolidated to a single path** (Hardening cleanup #1,
   migration 008). `users.is_platform_admin` is the **sole** enforcement
   path. `staff_roles` is now operational-roles-only — `grant_staff_role`
   rejects the `platform_admin` role string with `DomainError::InvalidInput`,
   and the `staff_roles_role_check` CHECK constraint at the database layer
   prevents any direct INSERT bypassing the service. Anchor doc-comment
   lives on `domain::domain::auth::types::UserRow::is_platform_admin`.
3. **`bcrypt` admin PINs are documented but unused.** `.env.example` claims
   the admin PIN fields are bcrypt-hashed at startup, but nothing in
   `server/src` ever reads `cfg.admin_pin` / `cfg.chocolatier_pin` /
   `cfg.supplier_pin`. The principal `platform_admin` therefore runs entirely
   through `users.is_platform_admin`; the PIN fields would belong to a fourth
   principal type that does not exist today. Out of scope for this matrix
   but worth recording.
