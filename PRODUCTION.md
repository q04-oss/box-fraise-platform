# Box Fraise Platform — Production Readiness

The pre-launch checklist + the environment-variable reference. Use this as
a gate before exposing the API to a real user.

Sister documents:
- `deploy/DEPLOY.md` — how to roll the binary onto a VPS.
- `deploy/BACKUP.md` — backup strategy and restore procedure.
- `deploy/INCIDENT_RESPONSE.md` — on-call playbook.
- `deploy/SECRETS_ROTATION.md` — per-secret rotation cadence.
- `HARDENING.md` — what the 12-section hardening pass shipped, and what's deferred.

---

## Pre-launch checklist

### Required before first real user

- [ ] `SOULTOKEN_SIGNING_KEY_HEX` generated via `scripts/generate_ed25519_key.sh` and set.
- [ ] `SOULTOKEN_VERIFYING_KEY_HEX` set to the public key derived from the signing key (server logs the derived value on first boot — `AppState::new` cross-checks).
- [ ] `JWT_SECRET` set (≥ 32 chars, random — `openssl rand -base64 48`).
- [ ] `STAFF_JWT_SECRET` set (separate from `JWT_SECRET`).
- [ ] `FRAISE_HMAC_SHARED_KEY` set.
- [ ] `SOULTOKEN_HMAC_KEY` set (display-code derivation).
- [ ] `ALLOWED_ORIGINS` set to the actual production frontend domain(s).
- [ ] `SENTRY_DSN` set.
- [ ] UptimeRobot configured to ping `/health` every 5 minutes.
- [ ] DigitalOcean Spaces bucket created (private; AES-256 SSE; deny-public ACL).
- [ ] `SPACES_*` env vars set.
- [ ] SSL certificate provisioned via certbot (`certbot --nginx -d api.boxfraise.com`).
- [ ] UFW firewall active, only ports 22 / 80 / 443 open.
- [ ] Database backups verified (Railway daily snapshots OR `pg_dump` cron — see `deploy/BACKUP.md`).
- [ ] `app_user_prod` role created in the database (migration 003).
- [ ] `APP_USER_DATABASE_URL` set so the application connects as the non-superuser role and RLS enforces.
- [ ] `/metrics` endpoint restricted to internal network (nginx loopback allowlist already in `deploy/nginx.conf`).

### Required before first business onboarding

- [ ] Stripe Identity configured (`STRIPE_SECRET_KEY` + `STRIPE_WEBHOOK_SECRET`).
- [ ] Background-check provider configured (provider stub today — finalize before live identity flow).
- [ ] Staff roles assigned to at least 2 `delivery_staff`.
- [ ] At least 2 `attestation_reviewer` roles assigned.
- [ ] First business registered and beacon configured.

### Required before public launch

- [ ] Stripe Terminal configured (if POS mode enabled).
- [ ] Push-notification certificates configured (Apple Push, Expo).
- [ ] Privacy policy published.
- [ ] Terms of service published.

---

## Scale thresholds

Don't pre-build for these — implement when triggered. See `ROADMAP.md`
Phase 9 for the full list. In one line each:

- **Kafka**: queue depth > 1 000/min sustained 7 days → introduce.
- **Sharding**: `presence_events` > 100 M rows → shard by `business_id`.
- **Kubernetes**: VPS CPU > 70% sustained → migrate.
- **Multi-region**: users in 3+ regions → replicate.
- **CDN**: > 1 TB/month static bandwidth → CloudFront or Bunny.
- **Read replicas**: analytics queries affecting API latency → split.

---

## Environment variables reference

Canonical source: `server/.env.example`. This table groups them by surface.

### Required

| Variable                          | Description                                                | Example / generator                          |
|-----------------------------------|------------------------------------------------------------|----------------------------------------------|
| `DATABASE_URL`                    | PostgreSQL connection string.                              | `postgres://user:pw@host:5432/fraise`        |
| `JWT_SECRET`                      | HS256 key for user JWTs (≥ 32 chars).                      | `openssl rand -base64 48`                    |
| `STAFF_JWT_SECRET`                | HS256 key for staff JWTs (≥ 32 chars).                     | `openssl rand -base64 48`                    |
| `ADMIN_PIN`                       | Full-admin pin (≥ 8 chars, not all same char).             | `openssl rand -base64 12`                    |
| `CHOCOLATIER_PIN`                 | Catalog/order management pin.                              | same                                         |
| `SUPPLIER_PIN`                    | Read-only-orders pin.                                      | same                                         |
| `STRIPE_SECRET_KEY`               | Stripe API key (`sk_test_...` / `sk_live_...`).            | dashboard.stripe.com → API keys              |
| `STRIPE_WEBHOOK_SECRET`           | Stripe webhook signing secret (`whsec_...`).               | dashboard.stripe.com → Webhooks              |
| `SOULTOKEN_HMAC_KEY`              | HMAC key for display-code derivation (≥ 32 chars).         | `openssl rand -base64 48`                    |
| `SOULTOKEN_SIGNING_KEY_HEX`       | Ed25519 private key (64 hex chars / 32 bytes).             | `scripts/generate_ed25519_key.sh`            |
| `SOULTOKEN_VERIFYING_KEY_HEX`     | Ed25519 public key (64 hex chars). Must match private key. | logged on first boot                         |

### Optional

| Variable                          | Description                                                | Default / behaviour without                  |
|-----------------------------------|------------------------------------------------------------|----------------------------------------------|
| `JWT_SECRET_PREVIOUS`             | Previous user-JWT key during rotation window.              | none                                         |
| `STAFF_JWT_SECRET_PREVIOUS`       | Previous staff-JWT key during rotation window.             | none                                         |
| `REVIEW_PIN`                      | Apple App Review demo PIN.                                 | none                                         |
| `APP_USER_DATABASE_URL`           | Production-only DB URL connecting as non-superuser.        | falls back to `DATABASE_URL` (RLS inert)     |
| `REDIS_URL`                       | Redis connection string. Required before scaling.          | in-process nonce cache (single instance)     |
| `FRAISE_HMAC_SHARED_KEY`          | iOS HMAC request-signing key.                              | iOS clients without per-device key rejected  |
| `APPLE_TEAM_ID` / `APPLE_KEY_ID` / `APPLE_CLIENT_ID` / `APPLE_PRIVATE_KEY` | Apple Sign-In quartet — all four required together. | Apple Sign In disabled                       |
| `RESEND_API_KEY`                  | Resend mail-sending API key.                               | emails skipped silently                      |
| `ANTHROPIC_API_KEY` / `ANTHROPIC_API_BASE_URL` | Anthropic API for Dorotka.                  | `/api/dorotka/ask` returns 500               |
| `CLOUDINARY_*`                    | Legacy Cloudinary trio.                                    | unused (stale env hint)                      |
| `SQUARE_*`                        | Square POS integration.                                    | Square integration disabled                  |
| `API_BASE_URL`                    | Public-facing base URL for transactional emails.           | `http://localhost:3001`                      |
| `PORT`                            | Server bind port.                                          | 3001                                         |
| `PLATFORM_FEE_BIPS`               | Platform fee in basis points.                              | 500 (5 %)                                    |
| `APP_STORE_ID`                    | Apple App Store numeric ID for fallback page.              | none                                         |
| `OPERATOR_EMAIL`                  | Operational alerts inbox.                                  | none                                         |
| `SENTRY_DSN`                      | Sentry project DSN (Hardening §4).                         | Sentry disabled — local logs only            |
| `SPACES_ACCESS_KEY` / `SPACES_SECRET_KEY` / `SPACES_BUCKET` / `SPACES_ENDPOINT` / `SPACES_REGION` | DigitalOcean Spaces (Hardening §3). | `/api/staff/visits/:id/evidence/*` returns 503 |
| `ALLOWED_ORIGINS`                 | Comma-separated CORS allow-list (Hardening §6).            | `http://localhost:3000`                      |
| `DB_MAX_CONNECTIONS` / `DB_MIN_CONNECTIONS` / `DB_ACQUIRE_TIMEOUT_SECS` | Postgres pool sizing (Hardening §6). | 20 / 2 / 5                                   |

---

## Performance baselines

To be established with `k6` against staging before launch. Targets:

- **p99 latency**: < 500 ms on every endpoint.
- **error rate**: < 0.1 % sustained.
- **DB pool utilisation**: < 80 %.
- **`/metrics` scrape**: < 100 ms p99.
- **SSE keepalive overhead**: negligible (axum default).
