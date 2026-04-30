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
- **Square webhook signature** — `validate_webhook` verifies HMAC-SHA256 over `notification_url + body`, base64-encoded, constant-time; server refuses to start if Square is configured without the signing key

## Health check

```
GET /health
```

Returns `200 {"status":"ok","db":"ok","redis":"ok"}` when all dependencies are healthy, `503 {"status":"degraded",...}` otherwise. This is the Railway health check target.
