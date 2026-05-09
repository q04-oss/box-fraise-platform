# CLAUDE.md

Read at session start. Provides instant context — don't crawl the codebase
to recover what's here.

## 1. Project identity

**box-fraise-platform** is the Rust backend implementing **BFIP v0.2.0**
(Box Fraise Identity Protocol) — verified-presence identity built on staff
attestation + Ed25519 soultokens. Powers the Box Fraise iOS app (verified
business presence, support bookings, orders) and is the platform layer for
forthcoming products (Whisked, business portal). Protocol spec lives in
`bfip/PROTOCOL.md` and `bfip/reference/cryptography.md`.

**Stack**: Rust 1.95 (MSVC on dev / GNU on Linux) · Axum 0.8 · sqlx 0.8 /
PostgreSQL 18 · deadpool-redis 7 · tokio · ring (HMAC + ECDSA) · ed25519-dalek
· utoipa (OpenAPI proc-macro) · sentry · prometheus-exporter.

## 2. Current state

- **Grade**: A — **9.01 weighted** / 9.02 straight (2026-05-08, see SCORECARD.md).
- **Tests**: 448 passing / 0 failing under both `fraise` (superuser) and
  `app_user_prod` (RLS-enforced). Run with `--test-threads=1` locally to avoid
  the cluster-wide `CREATE ROLE` race.
- **Last commit**: `2fd83f9` (scorecard 2026-05-08 A).
- **Migrations**: `001`–`012` applied. `001` schema, `002` RLS, `003` app_user,
  `004` analytics views, `005` compliance, `006` feature flags, `007` billing,
  `008` consolidate platform_admin, `009` rate limits, `010` concurrent fixes,
  `011` App Attest pubkey, `012` whisked menu/orders/pickup-codes.
- **Domain modules** (19 in `domain/src/domain/` + 2 server-only in
  `server/src/domain/`): attestation_tokens, attestations, auth,
  background_checks, beacons, **billing**, businesses, dorotka,
  identity_credentials, orders, platform_configuration, presence, soultokens,
  staff, support, users, verification_events, **whisked_menu**, **whisked_orders**
  · plus server-side `analytics` and `notifications` (SSE).

## 3. Architecture in one page

**Three crates** (workspace `Cargo.toml`):
- `domain` — pure logic, no axum imports (enforced by Cargo dep graph).
- `server` — HTTP layer (axum), depends on `domain` + `integrations`.
- `integrations` — third-party HTTP clients (Stripe, Anthropic, Resend, Expo,
  Spaces). Cycle-prevented: `integrations` does NOT depend on `domain`.

**Layer discipline**: `routes → service → repository`. Routes do auth +
parsing + response shaping; services hold business logic; repositories own
*all* SQL. Routes never call `sqlx::query*` directly (zero raw SQL in
`server/src/domain/**/routes.rs` as of 2026-05-08; `#![deny(clippy::disallowed_methods)]`
on each route file enforces this per-file).

**RLS enforcement-by-construction** (`domain/src/transaction.rs`):
`RlsTransaction` and `AdminRlsTransaction` cannot be obtained without setting
`app.user_id` / `app.is_admin` on the connection. Routes use these wrappers;
repositories take `&mut PgConnection` from inside the wrapper. Provably
exercised by the dedicated `test-rls` CI job.

**`audit::write` stays on `&PgPool`** (NOT inside `RlsTransaction`) — this
is intentional. Audit rows must survive transaction rollback so security
events are still recorded when the surrounding work fails. Don't move
audit writes into the transaction.

**`AppState`** (`server/src/app.rs:51-84`): `db` (PgPool), `cfg` (Arc<Config>),
`revoked` (JWT revocation cache), `nonces` (HMAC nonce dedup), `redis`
(Option<Pool>), `rate` + `dorotka_rate` (in-process IP limiters),
`user_rate_limiter` (Arc — Redis-backed, per-user post-auth), `http`
(reqwest), `event_bus`, `ed25519_key_pair` (Arc — soultoken signing),
`storage_client` (Option — DigitalOcean Spaces), `metric_handle`
(Prometheus), `event_tx` (broadcast<NotificationEvent>, 1024 buffer).

**Where to find things**:
- Domain logic: `domain/src/domain/{name}/service.rs`
- HTTP handlers: `server/src/domain/{name}/routes.rs`
- DB queries: `domain/src/domain/{name}/repository.rs`
- Types: `domain/src/domain/{name}/types.rs`
- Domain events: `domain/src/events.rs` (33 variants); SSE forwarding +
  audit + counters in `server/src/events.rs`
- Tests: `server/tests/{handler,integration,rls,contracts}.rs`; service-level
  tests inline as `#[sqlx::test]` in `domain/src/domain/*/service.rs`
- Migrations: `server/migrations/`
- Crypto primitives: `domain/src/crypto/{mod,ed25519}.rs`
- Middleware: `server/src/http/middleware/`

## 4. Key decisions (immutable)

- **Ed25519 soultoken signing** (NOT HMAC). Switched in BFIP v0.2.0; aggregated
  Ed25519 co-signing for attestation (`crypto/ed25519.rs::verify_aggregated_ed25519`).
- **Audit writes outside transaction** (intentional — they survive rollback so
  failed work still leaves a security trace).
- **`app_user_prod` for RLS enforcement** via `APP_USER_DATABASE_URL`. The
  default `fraise` superuser has BYPASSRLS so dev tests skip RLS unless
  `APP_USER_DATABASE_URL` is set.
- **Append-only tables** with `bf_prevent_modification` trigger:
  `audit_events`, `verification_events`, `presence_events`,
  `attestation_attempts`, `gift_box_history`. UPDATE/DELETE is also revoked
  for `app_user` (defence in depth).
- **`platform_admin` via `users.is_platform_admin` boolean only** (NOT via
  `staff_roles`). Migration `008_consolidate_platform_admin.sql` flattened this.
- **Webhook routes: 60s timeout group** (`app.rs:206-210`) — Stripe, identity,
  background-check, billing webhooks. Stripe retries on 5xx; budget for one
  retry inside the deadline.
- **LLM routes (Dorotka): 120s timeout group** (`app.rs:212-213`).
- **Default routes: 30s timeout** (`app.rs:184-204`).
- **Per-route timeout layers applied INSIDE the merge** so smaller doesn't
  override longer per-group budgets.
- **Constant-time HMAC compare** is canonical at `domain::crypto::constant_time_eq`;
  `integrations/src/stripe.rs` duplicates it (cycle-prevention) — keep in sync.

## 5. Current deferred items

From `HARDENING.md` deferred section (each has a TODO at the call site):

- **RLS per-request transaction wiring** (§2c) — naive middleware downgrades
  `SET LOCAL` to session scope on pooled connections. Fix is per-request
  transactions throughout. TODO: `server/src/app.rs` middleware-stack comment.
- **App Attest x5c chain validation against Apple root CA** (§3a) — leaf SPKI
  is extracted, full ECDSA verify is wired, but the cert chain is not pinned
  to Apple's root. TODO: `domain/src/auth/apple_attest.rs::parse_attestation`.
- **Full evidence-hash recompute** (§3) — server downloads the uploaded object
  and recomputes SHA-256 vs the client-supplied hash. Format/presence
  validation already enforced. TODO: `domain/src/domain/staff/service.rs::complete_visit`,
  `attestations/service.rs::initiate_attestation`.
- **`record_consent` at background-check initiation** (§9) — function exists,
  call site is TODO. `domain/src/domain/background_checks/service.rs`.
- **`constant_time_eq` shared utility crate** (§11) — would let `integrations`
  import the canonical impl from `domain::crypto` instead of duplicating.
  Blocked on a third utility crate (e.g. `box-fraise-common`). TODO:
  `integrations/src/stripe.rs` doc-comment.
- **`SOULTOKEN_HMAC_KEY` multi-version key rotation** (§1c) — version field is
  recorded; multi-key lookup deferred until rotation is operationally required.
  TODO: `domain/src/domain/soultokens/service.rs::derive_display_code`.

## 6. What's next

- **Atelier** (`q04-oss/atelier`) — internal AI dashboard consuming this
  platform's admin endpoints.
- **Whisked** — second product riding on this platform.
- **Customer-facing business portal** — first-class `business_reporting`
  domain (per-business funnel, soultoken issuance, support trends).
  Replaces admin-only `/api/admin/analytics/*` for businesses; lifts BFIP
  §12.1/§12.2 from Partial → Implemented.
- **VPS migration** — Phase 4. `app_user_prod` password rotation gates this.

## 7. How to start a session

PostgreSQL on `localhost:5432`, Redis on `6379`. From the repo root:

```powershell
# 1. Start the local stack
docker compose up -d

# 2. Toolchain + DB env (PowerShell)
$env:LIB = "C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\lib\x64"
$env:PATH += ";C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64"
$env:PATH += ";C:\Program Files\Rust stable MSVC 1.95\bin"
$env:DATABASE_URL = "postgresql://fraise:fraise@localhost:5432/fraise"
$env:REDIS_URL    = "redis://localhost:6379"

# 3. Run the suite (single-threaded — see note below)
cargo test --workspace -- --test-threads=1

# 4. Run under RLS (proves app_user_prod enforcement)
$env:APP_USER_DATABASE_URL = "postgresql://app_user_prod:CHANGE_ME_BEFORE_PRODUCTION@localhost:5432/fraise"
cargo test --workspace -- --test-threads=1
```

**Why `--test-threads=1`**: parallel sqlx::test workers race on cluster-wide
`CREATE ROLE app_user` in `002_rls.sql` despite the `IF NOT EXISTS` guard.
CI sidesteps this with a fresh Postgres per job. Locally, run sequentially.

**Why `flush_rate_limit_keys` in `handler.rs`**: shared Redis + per-test
fresh DB means `users.id = 1` collides across rate-limit tests. The two
adjacent rate-limit tests now flush `rate:*` at entry; don't remove.

## 8. Commit conventions

- `hardening/N:` — hardening pass section work
- `cleanup/N:` — cleanup items
- `fix/<area>:` — bug fix
- `feat/<area>:` — new feature
- `test/<area>:` — test additions
- `ci:` — CI / workflow changes
- `chore:` — housekeeping
- `scorecard:` — SCORECARD.md updates
- `docs/<area>:` — documentation only

Commit messages: subject ≤ 70 chars; body explains the *why*. Don't amend
published commits — always create a new commit if you need to fix.
