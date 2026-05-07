# Box Fraise Platform — Secrets Rotation Runbook

Hardening §10. Procedures for rotating each secret the platform reads. Use
this document as a checklist; record every rotation in the ops log so the
next person can confirm the cadence is being kept.

---

## Rotation schedule

| Secret                                | Cadence                | Notes                                              |
|---------------------------------------|------------------------|----------------------------------------------------|
| `JWT_SECRET`                          | every 90 days          | Invalidates all live sessions on swap.             |
| `STAFF_JWT_SECRET`                    | every 90 days          | Same shape as `JWT_SECRET`, separate key.          |
| `FRAISE_HMAC_SHARED_KEY`              | every 180 days         | iOS request-signing key. Coordinate with mobile.   |
| `SOULTOKEN_SIGNING_KEY_HEX` (Ed25519) | every 12 months        | New tokens use new key; old tokens age out.        |
| `SOULTOKEN_HMAC_KEY` (display codes)  | every 12 months        | Multi-version key store: see TODO in service.rs.   |
| `STRIPE_SECRET_KEY` / `STRIPE_WEBHOOK_SECRET` | on team departure | Or on suspected leak.                              |
| `SENTRY_DSN`                          | on team departure      | Sentry's project-level DSNs are leaky-by-design.   |
| Database password                     | every 90 days          | Pair with `app_user_prod` re-creation.             |
| `SPACES_ACCESS_KEY` / `SECRET_KEY`    | every 180 days         | DigitalOcean console rotation.                     |

---

## `JWT_SECRET` rotation

```sh
# 1. Mint new secret.
openssl rand -hex 32

# 2. Update env (Railway dashboard or `/opt/box-fraise-platform/.env`).
# 3. Restart server.
systemctl restart box-fraise-platform

# 4. All live JWTs are now invalid. Users get a 401 on next request and
#    the iOS client re-auths via Apple Sign In or magic link. Monitor
#    error rate for 30 minutes.
```

The codebase already supports a `JWT_SECRET_PREVIOUS` field — set the old
secret there during a graceful rotation window so in-flight tokens keep
verifying, then clear it after 1 hour.

---

## `SOULTOKEN_SIGNING_KEY_HEX` rotation (Ed25519)

```sh
# 1. Mint a new key pair locally.
scripts/generate_ed25519_key.sh
# → prints SOULTOKEN_SIGNING_KEY_HEX=...

# 2. Copy the printed signing key into env.
# 3. Restart — server logs the derived verifying key on first boot.
# 4. Copy that verifying key into SOULTOKEN_VERIFYING_KEY_HEX.
# 5. Restart again. AppState::new now passes the cross-check.
```

Existing soultokens stay valid (signed with the old key, verified against
the OLD verifying key — but they're never re-verified against the trust
registry). The trust registry endpoint serves the new public key
immediately. After 12 months every soultoken signed with the old key has
expired and the old key can be decommissioned.

---

## `SOULTOKEN_HMAC_KEY` rotation (display codes)

Display codes are HMAC-derived from the soultoken UUID. The schema already
records the HMAC key version on each soultoken row
(`display_code_key_version`).

**Multi-version key lookup is not yet wired up** — see the TODO in
`derive_display_code` (`domain/src/domain/soultokens/service.rs`). Until
that pass lands, rotating this key invalidates every existing display code.
Don't rotate it without coordinating with mobile; the iOS client caches
display codes for offline presentation.

---

## Database password rotation

```sh
# 1. Create the new role.
psql "$ADMIN_DATABASE_URL" <<SQL
CREATE USER app_user_prod_new WITH PASSWORD '<new-strong-password>';
GRANT app_user TO app_user_prod_new;
SQL

# 2. Update DATABASE_URL (or APP_USER_DATABASE_URL).
# 3. Restart server, verify /health returns "healthy".
systemctl restart box-fraise-platform

# 4. Drop the old role.
psql "$ADMIN_DATABASE_URL" <<SQL
DROP USER app_user_prod;
SQL

# 5. Rename the new role into place if desired (cosmetic).
psql "$ADMIN_DATABASE_URL" -c 'ALTER USER app_user_prod_new RENAME TO app_user_prod;'
```

---

## What this runbook doesn't cover yet

- Hardware-key rotation for App Attest / Apple Sign In private keys (those
  live with Apple; rotation is a developer-portal action, not a server-side
  one).
- Per-reviewer Ed25519 key rotation for attestation co-signs — see the
  TODO(BFAP) comment in `attestations::service::verify_signature`. Today
  the platform key signs on behalf of every reviewer.
