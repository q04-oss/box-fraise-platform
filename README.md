# box-fraise-platform

Rust/Axum backend for the Box Fraise platform — loyalty stamps, venue drink ordering via Square POS, Stripe Connect payments, end-to-end encrypted messaging keys, and a staff PWA for scanning customer QR codes.

## What's in this repo

```
server/          Rust/Axum API server
  src/
    domain/      Business logic, one subdirectory per domain
    http/        Middleware (HMAC signing, rate limiting) and extractors
    integrations/ Third-party clients (Stripe, Square, Resend, Expo Push)
    auth/        JWT, Apple Sign In, App Attest verification
    crypto.rs    AES-256-GCM for Square OAuth token encryption
  migrations/    sqlx migrations — run in order to reproduce full schema
  tests/         Integration tests (sqlx::test + testcontainers)
```

## Running locally

**Prerequisites:** Rust stable, Docker.

```bash
# Start PostgreSQL 18 and Redis 7
docker-compose up -d

# Copy and fill in required env vars
cp server/.env.example server/.env
# Edit server/.env — at minimum: DATABASE_URL, JWT_SECRET, STAFF_JWT_SECRET,
# STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET, ADMIN_PIN, CHOCOLATIER_PIN, SUPPLIER_PIN

# Run migrations
cd server
DATABASE_URL=postgres://fraise:fraise@localhost:5432/fraise \
  cargo run --bin sqlx -- migrate run

# Start the server
cargo run
# Listening on http://localhost:3001
```

The server reads `server/.env` on startup via `dotenvy`.

## Running migrations

```bash
cd server
sqlx migrate run          # apply pending migrations
sqlx migrate revert       # revert most recent migration
sqlx migrate info         # show migration status
```

Database URL is read from `DATABASE_URL` env var (or `server/.env`).

## Running tests

Tests require a running PostgreSQL instance. Redis is provided automatically via testcontainers (Docker must be running).

```bash
cd server
DATABASE_URL=postgres://fraise:fraise@localhost:5432/fraise cargo test
```

Each `#[sqlx::test]` spins up a fresh database, runs all migrations, and cleans up. Tests are fully isolated — no shared state between test functions.

To run a specific test file:
```bash
cargo test --test loyalty
cargo test --test auth
cargo test --test venue_drinks
cargo test --test square_webhook
```

## Schema

The full schema is reproducible from migrations alone:

```bash
# Fresh database from scratch
sqlx migrate run
```

`migrations/000_initial_schema.sql` captures the original platform schema. Migrations 001–007 add columns and tables incrementally.

## Key security properties

- **HMAC-SHA256 request signing** — every iOS request is signed with a per-device or shared key; the nonce is included in the signed message; nonces are tracked in Redis (or in-process) to prevent replay
- **App Attest** — attested devices sign every request with an ECDSA key stored in `device_attestations`; the server verifies the assertion against the stored public key
- **Append-only loyalty ledger** — `loyalty_events` has a DB trigger that rejects UPDATE and DELETE; balances are always derived from the full event history
- **Single-use tokens** — QR stamp tokens, email verification tokens, OAuth CSRF state, and NFC activation windows are all GETDEL (Redis) so replay is structurally impossible
- **AES-256-GCM at rest** — Square OAuth tokens are encrypted at the application layer before hitting the DB; the encryption key is an env var, not in the database
- **Separate staff JWT secret** — `STAFF_JWT_SECRET` is distinct from `JWT_SECRET`; a user token cannot be decoded as a `StaffClaims` token regardless of library bugs
- **Redis-backed JWT revocation** — logout and other security events write `SET fraise:revoked:{jti} 1 EX {ttl}` to Redis; every authenticated request checks `EXISTS fraise:revoked:{jti}` before reaching a handler; both user and staff tokens are checked; falls back to in-process store if Redis is unavailable
- **JWT secret rotation without forced logout** — set `JWT_SECRET_PREVIOUS=$OLD_SECRET` and `JWT_SECRET=$NEW_SECRET` and deploy; tokens signed with the old key verify against the previous-secret fallback and remain valid until they naturally expire; unset `JWT_SECRET_PREVIOUS` after one full token TTL (90 days for user tokens, 8h for staff); same procedure applies to `STAFF_JWT_SECRET_PREVIOUS`
- **Square webhook signature** — `validate_webhook` verifies HMAC-SHA256 over `notification_url + body`, base64-encoded, constant-time comparison via `subtle::ConstantTimeEq`; server refuses to start if Square is configured without the signing key

## Development

Install [just](https://github.com/casey/just): `cargo install just`

```bash
just test          # cargo test --workspace
just check         # cargo check + cargo clippy -D warnings
just audit         # cargo audit + cargo deny check
just ci            # check → test → audit (full local CI)
just drift         # check for schema/migration drift
just docs          # cargo doc --no-deps --open
just fuzz-hmac     # fuzz the HMAC verifier (requires nightly)
just fuzz-sanitise # fuzz the input sanitiser (requires nightly)
```

See [WORKFLOW.md](WORKFLOW.md) for the four-phase development process.

---

## Security testing

The two highest-risk surfaces have cargo-fuzz targets:

```bash
# Fuzz HMAC signing and constant-time comparison
cargo +nightly fuzz run hmac_verify

# Fuzz the Dorotka input sanitiser (null bytes, control chars, oversized inputs)
cargo +nightly fuzz run sanitise
```

Property-based tests (via proptest) run as part of the normal test suite:

```bash
cargo test --workspace  # includes proptest runs
```

---

## Health check

```
GET /health
```

Returns `200 {"status":"ok","db":"ok","redis":"ok"}` when all dependencies are healthy, `503 {"status":"degraded",...}` otherwise. This is the Railway health check target.
