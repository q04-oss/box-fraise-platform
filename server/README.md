# box-fraise-platform — server

Rust/Axum API server for the Box Fraise platform. Deployed on Railway via Docker.

## Running locally

```bash
# 1. Start Postgres and Redis (Docker recommended)
docker compose up -d

# 2. Copy and fill in secrets
cp .env.example .env
$EDITOR .env

# 3. Run migrations
DATABASE_URL=$(grep DATABASE_URL .env | cut -d= -f2) sqlx migrate run

# 4. Start the server
cargo run
```

Server listens on `http://localhost:3001` by default. Override with `PORT=`.

## Running tests

```bash
# Unit tests (no database required)
cargo test

# Integration tests (requires DATABASE_URL pointing at a test database)
DATABASE_URL=postgres://postgres:postgres@localhost:5432/fraise_test cargo test --test integration
```

The CI workflow (`.github/workflows/ci.yml`) runs all tests against a real Postgres service on every push.

## Domain structure

```
src/domain/
├── admin/          Admin operator endpoints (PIN-gated, bcrypt-verified)
├── art/            Artwork and portrait token marketplace
├── auth/           Authentication: Apple Sign In, magic link, email+password
├── businesses/     Business registration and verification
├── campaigns/      Ad campaigns and impressions
├── catalog/        Product catalog (varieties, pricing, stock)
├── contracts/      Employment contracts between users and businesses
├── devices/        Cardputer device registration, attestation, role assignment
├── dorotka/        Dorotka AI assistant (Anthropic, host-scoped system prompt)
├── gifts/          Gift sending and single-use claim tokens
├── keys/           API key management
├── loyalty/        Loyalty programme: QR stamps, NFC stickers, steep balance
├── memberships/    Membership tiers, payment intents, fund contributions
├── messages/       Platform messaging
├── nfc/            NFC connection tracking
├── orders/         Order creation, payment, collection
├── payments/       Stripe webhook handlers
├── popups/         Pop-up event RSVPs
├── portal/         Portal access purchase and management
├── search/         User search
├── squareoauth/    Square POS OAuth integration
├── staff_web/      Staff web PWA (HTML, cookie-authed)
├── tokens/         Evening and portrait tokens
├── tournaments/    Tournament entry and management
├── users/          User profile, social graph, notifications
├── venue_drinks/   In-venue drink ordering (Square POS push)
└── ventures/       Venture funding
```

Each domain follows a consistent four-file structure:

| File | Responsibility |
|---|---|
| `routes.rs` | Axum handlers, routing, extractor composition |
| `service.rs` | Business logic, orchestration |
| `repository.rs` | SQL queries (all parameterised — no string interpolation) |
| `types.rs` | Request/response structs, `sqlx::FromRow` row types |

## Middleware ordering

Middleware is applied innermost-last in Axum, so execution order on a request is:

```
log_rejections (enter)
  └─ rate_limit::check (enter)
       └─ hmac::validate (enter)
            └─ handler (runs)
            └─ hmac::validate (exit)
       └─ rate_limit::check (exit)
  └─ log_rejections (exit — emits warn! on 401/403)
```

Outer layers (TraceLayer, SetResponseHeaderLayer, CompressionLayer) wrap all of the above.

## Auth extractors

| Extractor | Yields | Use when |
|---|---|---|
| `RequireUser` | `UserId` | Any authenticated user action |
| `RequireClaims` | `Claims` (with `jti`) | Logout (needs JWT ID for revocation) |
| `OptionalAuth` | `Option<UserId>` | Public endpoints that personalise for logged-in users |
| `RequireStaff` | `StaffClaims` (with `business_id`) | Staff-only business operations |
| `RequireDevice` | `DeviceInfo` (with `business_id`) | Cardputer device endpoints |

**Rule:** never use `Option<T>` on an extractor when the endpoint requires auth. The compiler enforces this — `RequireUser` is `UserId`, not `Option<UserId>`.

## Security model

- **iOS requests** are HMAC-SHA256 signed over `method + path + timestamp + nonce + body`. Replay prevention uses Redis `SET NX EX` (distributed) or an in-process HashMap (single instance).
- **App Attest** assertions are verified per-request for attested devices.
- **JWTs** are revocable via Redis before their 90-day TTL expires.
- **Admin PINs** are bcrypt-hashed at startup (`AppState::new()`). Raw values are never retained.
- **All SQL** is parameterised — no string interpolation anywhere in the codebase.
- **Audit log** is append-only with DB-level triggers preventing modification.
- **Rate limiting** is per-IP (global), per-email (magic link, password reset), and per-IP (Dorotka AI).

See `src/http/middleware/hmac.rs` for the full HMAC verification flow with threat model commentary.

## Key files

| File | Purpose |
|---|---|
| `src/lib.rs` | Server startup: tracing, config, DB pool, migrations, bind |
| `src/app.rs` | `AppState` definition, `build()` router, middleware stack |
| `src/config.rs` | All env var loading with `SecretString` for sensitive values |
| `src/error.rs` | `AppError` enum → HTTP status + JSON body |
| `src/audit.rs` | `audit::write()` — append-only security event log |
| `src/auth/mod.rs` | JWT sign/verify, revocation (Redis + in-process fallback) |
| `src/http/middleware/hmac.rs` | HMAC validation + App Attest + nonce deduplication |
| `src/http/middleware/rate_limit.rs` | Per-IP rate limiter (Redis + in-process fallback) |
| `src/http/extractors/auth.rs` | `RequireUser`, `RequireStaff`, `RequireDevice`, etc. |
| `migrations/README.md` | Migration history and schema domain groupings |
| `.env.example` | Every environment variable with description and source |

## Adding a new endpoint

1. Add the route to `domain/<name>/routes.rs` using the appropriate extractor.
2. Put business logic in `service.rs`, SQL in `repository.rs`, types in `types.rs`.
3. Register the router in `app.rs` with `.merge(crate::domain::<name>::routes::router())`.
4. If the endpoint mutates state, add an `audit::write()` call.
5. Run `/audit` (Claude Code slash command) before opening a PR.
