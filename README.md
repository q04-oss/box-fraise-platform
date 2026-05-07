# Box Fraise Platform

The backend for Box Fraise — a premium chocolate-covered-strawberry
platform built on the world's most rigorous human-identity verification
protocol. Rust / Axum / PostgreSQL / Redis.

## Architecture

- **16 domains** implementing every section of BFIP v0.2.0.
- **Ed25519** cryptographic signing for soultokens and attestation co-signs.
- **Row Level Security** policies on 38 tables (3 PostgreSQL roles).
- **SSE** real-time notifications.
- **DigitalOcean Spaces** (S3-compatible) for evidence storage.
- **Prometheus + Sentry** for observability.

## Protocols

- **BFIP v0.2.0** — Box Fraise Identity Protocol — `bfip/PROTOCOL.md`
- **BFMP v0.1.0** — Box Fraise Mesh Protocol — repo `q04-oss/bfmp`
- **BFAP v0.1.0 (stub)** — Box Fraise Agent Protocol — `bfap/PROTOCOL.md`

## Repository layout

```
domain/         Business logic. One subdirectory per domain. No HTTP types.
integrations/   Third-party clients (Stripe, Anthropic, Resend, Spaces, ...).
server/         Axum API server.
  src/
    app.rs        AppState, router, middleware stack.
    domain/       Per-domain HTTP handlers (one subdirectory each).
    http/         Middleware (CORS, HMAC, rate limit, security headers, ...).
    notifications.rs   NotificationEvent enum (SSE).
    tasks/        Background daemons (retention pruning).
    events.rs     Domain-event handler (audit + counters + notifications).
  migrations/   sqlx migrations 001..007 — schema, RLS, app user, analytics views,
                compliance, feature flags, billing.
  tests/        Integration tests via #[sqlx::test] (fresh DB per test).
fuzz/           Cargo-fuzz targets (HMAC verify, sanitiser).
bfip/           Protocol spec + cryptographic-primitives reference.
bfap/           BFAP draft specification.
deploy/         nginx.conf, systemd unit, runbooks (DEPLOY, BACKUP, INCIDENT_RESPONSE, SECRETS_ROTATION).
docs/           ACCESS_CONTROL_MATRIX.md and other design docs.
scripts/        Operator scripts (e.g. generate_ed25519_key.sh).
```

## Quick start (local development)

Prerequisites: Rust stable, Docker.

```sh
# Postgres + Redis
docker compose up -d

# Environment
cp server/.env.example server/.env
# Fill in JWT_SECRET, STAFF_JWT_SECRET, ADMIN_PIN, CHOCOLATIER_PIN,
# SUPPLIER_PIN, STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET,
# SOULTOKEN_HMAC_KEY. Mint Ed25519 keys with:
scripts/generate_ed25519_key.sh

# Migrations
DATABASE_URL=postgresql://fraise:fraise@localhost:5432/fraise \
  sqlx migrate run --source server/migrations

# Tests (fresh DB per test via sqlx::test)
DATABASE_URL=postgresql://fraise:fraise@localhost:5432/fraise \
REDIS_URL=redis://localhost:6379 \
  cargo test --workspace

# Run the server
cd server && cargo run
# Listening on http://localhost:3001
```

## Deployment

See `deploy/DEPLOY.md` for the VPS bring-up procedure. `Dockerfile` ships
the binary as a multi-stage build; `docker-compose.yml` `--profile full`
runs the whole stack locally.

```sh
docker compose --profile full up      # full stack including server
```

## Hardening status

12-section hardening pass complete. See `HARDENING.md` for the per-section
checklist of what shipped and what's deferred.

## Production readiness

Pre-launch checklist + the canonical environment-variable reference live
in `PRODUCTION.md`.

## Roadmap

`ROADMAP.md` traces the project arc from current state through the iOS
integration, the in-store terminal, BFAP, and Web3 settlement.

## Key security properties

- **Ed25519 soultoken signing** — every soultoken signature can be
  verified offline against the public key served at
  `GET /api/trust-registry/public-key`.
- **Aggregated Ed25519 attestation co-signing** — both reviewer signatures
  are verified before the user is promoted to `attested`.
- **Append-only audit log** — `audit_events`, `verification_events`,
  `attestation_attempts`, and 4 other tables are protected by the
  `bf_prevent_modification` trigger; rows can never be UPDATE'd or DELETE'd.
- **HMAC-SHA256 iOS request signing** — every iOS request carries a
  per-device HMAC over `method + path + ts + nonce + body`; nonces are
  deduped through Redis (or in-process fallback) to prevent replay.
- **Single-use tokens** — magic links, attestation tokens, and NFC chip
  taps are all single-consumption; replayed tokens are structurally rejected.
- **AES-256-GCM at rest** — Square OAuth tokens are encrypted at the
  application layer.
- **Separate user / staff JWT secrets** — a user token can never be
  decoded as a staff claim.
- **Redis-backed JWT revocation** — every authenticated request checks
  `EXISTS fraise:revoked:{jti}` against Redis (with in-process fallback).
- **JWT secret rotation without forced logout** — set
  `JWT_SECRET_PREVIOUS=$OLD_SECRET` and `JWT_SECRET=$NEW_SECRET`; tokens
  signed with either verify until the old one's natural TTL.
- **Constant-time signature comparison** — every HMAC tag check goes
  through `domain::crypto::constant_time_eq`.

## Health check

```
GET /health
```

Returns `200 {"status":"healthy", "database":"ok", "redis":"ok",
"storage":"configured"|"not_configured", "version":"<crate version>"}`
when DB is up; `200 {"status":"degraded", ...}` when only Redis is down
(does not page UptimeRobot); `503 {"status":"unhealthy", ...}` when DB
is unreachable.

## Development workflow

```sh
just test          # cargo test --workspace
just check         # cargo check + cargo clippy -D warnings
just audit         # cargo audit + cargo deny check
just ci            # full local CI: check → test → audit
just drift         # check for schema/migration drift
just docs          # cargo doc --no-deps --open
just fuzz-hmac     # fuzz HMAC verifier (requires nightly)
just fuzz-sanitise # fuzz Dorotka input sanitiser (requires nightly)
```

See `WORKFLOW.md` for the four-phase development process.

## Security testing

Two highest-risk surfaces have cargo-fuzz targets:

```sh
cargo +nightly fuzz run hmac_verify
cargo +nightly fuzz run sanitise
```

Property-based tests via `proptest` run as part of `cargo test`.

## License

See repo settings.
