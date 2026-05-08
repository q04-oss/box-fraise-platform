# Box Fraise Platform — Hardening Checklist

What the 12-section hardening pass shipped, what it explicitly deferred, and
which TODOs are tracked in code or docs. Read alongside `ROADMAP.md`
(future work) and `PRODUCTION.md` (gating launch).

Every section's last commit is in `ROADMAP.md` Phase 2.

---

## Completed

### §1 — Cryptographic upgrades

- [x] Ed25519 key infrastructure (`domain::crypto::ed25519`).
- [x] Ed25519 soultoken signing — replaces HMAC-SHA256.
- [x] Aggregated Ed25519 attestation co-signing — both reviewer signatures verified before approval.
- [x] PRF formalisation — beacon UUID + witness HMAC documented with formal security statements.
- [x] Cryptographic audit (clean — `subtle` and `bcrypt` removed as unused).
- [x] BFIP v0.2.0 published (`bfip/PROTOCOL.md`).

### §2 — RLS + access control

- [x] Formal access control matrix (`docs/ACCESS_CONTROL_MATRIX.md`).
- [x] RLS policies on 38 tables (`002_rls.sql` — 73 policies, 34 RLS-enabled tables).
- [x] RLS infrastructure: `app_user` / `app_readonly` / `app_admin` roles, `set_rls_user_context` helper, `APP_USER_DATABASE_URL` config field.
- [ ] Per-request transaction refactor — RLS policies are inert until the application connects as `app_user` AND `SET LOCAL app.user_id` is wired to the request lifecycle. Documented in the §2c TODO at `server/src/app.rs`.

### §3 — S3 evidence storage

- [x] `StorageClient` (DigitalOcean Spaces) with upload, presigned URLs, delete.
- [x] Server-side SHA-256 evidence hash computation.
- [x] `POST /api/staff/visits/:id/evidence` — multipart upload.
- [x] `GET /api/staff/visits/:id/evidence/url` — presigned URL.
- [x] Private bucket configuration documented; ACL `private` + AES-256 SSE on every PUT.
- [x] Evidence-hash format + presence enforcement at `complete_visit` and `initiate_attestation` (cleanup #5) — server rejects URI-without-hash and any hash that isn't 64-char lowercase hex. Full server-side recompute (download from S3) deferred — too expensive per request; tracked in deferred-items table.
- [ ] Bucket lifecycle policies (operational task).

### §4 — Observability

- [x] axum-prometheus middleware on every route.
- [x] `GET /metrics` (no auth — internal only).
- [x] 7 BFIP domain counters in `events.rs`.
- [x] Sentry integration via `sentry-tracing` (no-op without `SENTRY_DSN`).
- [x] `/health` enhanced: `{status, database, redis, storage, version}` with `healthy` / `degraded` / `unhealthy`.
- [x] Structured error responses (`{error: <slug>, message: <text>}`).
- [x] Startup config-summary log line.
- [ ] UptimeRobot configured (operational — set up account).
- [ ] Grafana + Prometheus on VPS (post-migration).
- [ ] Alert rules (post-migration).

### §5 — Product analytics

- [x] 8 analytics query functions (`server/src/domain/analytics/queries.rs`).
- [x] 8 admin-only analytics routes.
- [x] 3 Metabase-ready views (`004_analytics_views.sql`) granted to `app_readonly`.
- [ ] Metabase installed (post-VPS migration).

### §6 — API hardening

- [x] CORS lockdown (explicit `allowed_origins`, no wildcard).
- [x] Global 30 s request timeout.
- [x] Connection pool tuning configurable from env.
- [x] Dorotka soultoken gating — `/api/dorotka/ask` requires active soultoken.
- [x] `Retry-After: 60` on every 429 response.
- [x] CSP per-response nonce middleware.
- [x] `X-Content-Type-Options`, `X-Frame-Options`, `Referrer-Policy`, `Permissions-Policy` (now with `camera=()`).
- [ ] Per-endpoint rate-limit tuning — TODO block in `rate_limit.rs`.

### §7 — SSE real-time notifications

- [x] `NotificationEvent` enum (7 variants).
- [x] `broadcast::Sender` on `AppState` (capacity 1024).
- [x] `GET /api/notifications/stream` SSE endpoint.
- [x] User-scoped event filtering (`target_user_id` matches caller, or 0 broadcasts to all).
- [x] 6 domain-event match arms publish notifications.
- [x] `business_id` in `OrderReady` populated from the `orders` row at handler time (cleanup #6).
- [x] `display_code` in `SoultokenIssued` carried through the domain event (cleanup #6).

### §8 — Infrastructure

- [x] Multi-stage `Dockerfile` (rust:1.95-slim-bookworm → debian:bookworm-slim).
- [x] `.dockerignore`.
- [x] `docker-compose.yml` `--profile full` server scaffold.
- [x] CI `docker-build` job with GHA layer cache.
- [x] `deploy/nginx.conf` with SSE long-timeout block + `/metrics` allow-list.
- [x] `deploy/box-fraise-platform.service` systemd unit with security hardening.
- [x] `deploy/DEPLOY.md` runbook.
- [ ] VPS provisioned (post-hardening).
- [ ] Staging environment (post-VPS).

### §9 — Data compliance

- [x] `consent_records` table (`005_compliance.sql`).
- [x] `DELETE /api/users/me` — GDPR right to erasure (anonymise in place).
- [x] `GET /api/users/me/export` — GDPR right to portability.
- [x] `record_consent` function.
- [x] Daily retention pruning daemon (`server/src/tasks/retention.rs`).
- [x] `deploy/BACKUP.md` (RTO 4 h, RPO 24 h, restore procedure).
- [ ] `record_consent` wiring at background-check initiation (TODO).
- [ ] Automated backup cron (post-VPS).

### §10 — Operational

- [x] Consent wired into auth flows (Apple Sign-In + magic link, non-blocking on failure).
- [x] Feature flags (`006_feature_flags.sql` — 5 forward-looking flags seeded; `is_feature_enabled` resolution: global → allow-list → percent rollout).
- [x] Billing scaffolding (`007_billing.sql` — three tiers).
- [x] `admin_ban_user` / `admin_unban_user` — bans revoke active soultokens.
- [x] `deploy/SECRETS_ROTATION.md` (per-secret cadence + procedures).
- [x] `deploy/INCIDENT_RESPONSE.md` (P0–P3 playbook).
- [ ] Stripe subscription webhook (post-iOS).
- [ ] Per-endpoint rate-limit tuning (deferred).

### §11 — Protocol updates

- [x] `constant_time_eq` consolidated — `domain::crypto::constant_time_eq` is canonical, `hmac.rs` re-exports it.
- [x] BFIP §22 stub (agent delegation credentials).
- [x] BFAP v0.1.0 stub specification (`bfap/PROTOCOL.md`).
- [x] Trust-registry endpoint reports `bfip_version: "0.2.0"`.
- [ ] `constant_time_eq` utility crate — would let `integrations` import canonical impl instead of duplicating.

### §12 — Documentation (this commit)

- [x] `ROADMAP.md` — full project arc.
- [x] `PRODUCTION.md` — pre-launch checklist + env-var reference.
- [x] `HARDENING.md` — this file.
- [x] `README.md` — refreshed.
- [x] Final scorecard run.

---

## Deferred items

Items the hardening pass identified but didn't ship in-section. Each has a
TODO marker in code or docs at the relevant call site. Listed by impact,
not by section:

| Item                                                          | Section | Where the TODO lives                                            |
|---------------------------------------------------------------|---------|-----------------------------------------------------------------|
| RLS per-request transaction wiring                            | §2c     | `server/src/app.rs` middleware-stack comment                    |
| ~~App Attest full-crypto verify in `record_beacon_dwell` / `record_nfc_tap`~~ — **shipped Grade A item 1**: per-device public-key DER persisted on `identity_credentials` (migration 011), `enforce_assertion` now performs full ECDSA-P256 verification against the registered key for every gated request. Apple-root x5c chain validation in `parse_attestation` remains a follow-up. | §3a | `domain/src/auth/apple_attest.rs::enforce_assertion`, `domain/src/domain/identity_credentials/repository.rs::{register_app_attest_key,get_app_attest_public_key}` |
| Full evidence-hash recompute (download object + SHA-256 vs client hash). Format/presence validation already lands in cleanup #5. | §3      | `domain/src/domain/staff/service.rs::complete_visit` + `attestations/service.rs::initiate_attestation` |
| `record_consent` at background-check initiation               | §9      | `domain/src/domain/background_checks/service.rs`                |
| `constant_time_eq` shared utility crate                       | §11     | `integrations/src/stripe.rs` doc-comment                        |
| `SOULTOKEN_HMAC_KEY` multi-version key rotation               | §1c     | `domain/src/domain/soultokens/service.rs::derive_display_code`  |
| Stripe billing subscription webhook                           | §10     | `business_subscriptions` table left empty                       |
| ~~Per-endpoint rate-limit tuning (config rows seeded in migration 009; per-user keying requires post-auth middleware refactor)~~ — **shipped Grade A item 3**: per-user limiter wired into `attestations`, `background_checks`, `identity`, `dorotka` routes; reads limit values from `platform_configuration` so ops retune without redeploy. | §6 / §10 | `server/src/http/middleware/user_rate_limit.rs`                 |

None of the above block production launch — they're either operational
work, follow-up refactors, or fail-safe defaults that require the next
section of work to make material.
