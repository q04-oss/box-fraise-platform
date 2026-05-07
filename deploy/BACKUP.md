# Box Fraise Platform — Backup Strategy

Hardening §9. Backup, retention, and restore procedures for the production
PostgreSQL database. This document is operational; the application enforces
nothing here itself.

---

## Database backups

### Railway (current)

Railway PostgreSQL on a paid plan ships automatic daily backups with
**7-day retention**. No additional configuration required for the platform
to be backed up; verify the project's Storage tab shows recent snapshots.

### VPS deployment (future)

When migrating off Railway, implement the following on the VPS:

#### 1. Daily `pg_dump` via cron

`/etc/cron.d/box-fraise-platform-backup`:

```cron
0 2 * * * boxfraise pg_dump "$DATABASE_URL" | gzip > /backups/$(date +\%Y\%m\%d).sql.gz
```

`DATABASE_URL` should come from `/opt/box-fraise-platform/.env` (source it in the cron command or use `EnvironmentFile` from a systemd timer instead of cron).

#### 2. Local 7-day retention

```sh
find /backups -name "*.sql.gz" -mtime +7 -delete
```

Either tail this onto the cron command or schedule it as its own daily job.

#### 3. Off-site upload to DigitalOcean Spaces

`s3cmd` configured with `SPACES_ACCESS_KEY` / `SPACES_SECRET_KEY`:

```sh
s3cmd put /backups/$(date +%Y%m%d).sql.gz \
         s3://box-fraise-backups/$(date +%Y%m%d).sql.gz
```

The bucket should be in a different region from the application VPS so a
data-centre-level failure doesn't take both copies down.

#### 4. Spaces-side 90-day lifecycle policy

Configure the bucket's lifecycle rule to expire `*.sql.gz` after 90 days.
This is set once at bucket creation; the daily upload then runs unattended.

---

## Recovery targets

- **RTO** (Recovery Time Objective): 4 hours from incident → restored service.
- **RPO** (Recovery Point Objective): 24 hours from incident → maximum acceptable data loss (the daily backup cadence).

If the business requires tighter RPO than 24h, move to continuous WAL
shipping (`pg_basebackup` + WAL archive to Spaces).

---

## Restore procedure

1. Stop the server:
   ```sh
   systemctl stop box-fraise-platform
   ```

2. Drop and recreate the database (preserves the role):
   ```sh
   psql -d postgres -c "DROP DATABASE fraise;"
   psql -d postgres -c "CREATE DATABASE fraise OWNER fraise;"
   ```

3. Restore from the most recent backup:
   ```sh
   gunzip -c /backups/20260601.sql.gz | psql "$DATABASE_URL"
   ```

4. Verify schema integrity by running the test suite against the restored DB
   (set `DATABASE_URL` to the restored DB and `cargo test --workspace`). All
   376+ tests should pass.

5. Restart:
   ```sh
   systemctl start box-fraise-platform
   curl -sS https://api.boxfraise.com/health | jq .
   ```

---

## Critical data retention periods

These are documented in the source schema (migration 005 `COMMENT ON`
statements) and enforced in code (`tasks/retention.rs`) where automated
pruning applies.

| Data                          | Retention              | Enforced by                              |
|-------------------------------|------------------------|------------------------------------------|
| `audit_events`                | 7 years (legal)        | manual operational task                  |
| `attestation_attempts`        | 7 years                | trigger blocks UPDATE/DELETE             |
| `soultokens` (after revoked)  | 7 years                | manual                                   |
| `background_checks`           | 12 months after expiry | manual                                   |
| `verification_events`         | per user lifecycle     | anonymised on user erasure               |
| `jwt_revocations`             | until expiry + 24 h    | `tasks/retention.rs` daily               |
| `magic_link_tokens`           | until used + 1 h       | `tasks/retention.rs` daily               |
| `consent_records`             | indefinite (legal)     | append-only by service-layer convention  |
| `users`                       | indefinite             | anonymised on erasure (never hard-deleted) |

The two automated entries are the only places the application removes rows on
its own. Everything else relies on operator action; don't enable a "tidy up"
cron without consulting Legal.
