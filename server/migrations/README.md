# Migrations

## Current schema: BFIP v0.1.2

| File | What it establishes |
|---|---|
| `000_drop_legacy_tables.sql` | Drop pre-BFIP tables that are replaced by the new schema. |
| `001_bfip_schema.sql` | Full BFIP v0.1.2 identity protocol schema — 37 tables. |

## Archive

Migrations `000_initial_schema.sql` through `029_drop_stale_tables.sql` (the pre-BFIP schema history) have been moved to `archive/`. They are retained for reference and audit trail but are no longer applied — the BFIP schema supersedes them entirely.

## Running migrations locally

```bash
cd server
sqlx migrate run --database-url "$DATABASE_URL"
```

For Railway:

```bash
railway run sqlx migrate run
```

## Adding a new migration

```bash
sqlx migrate add --source migrations <descriptive_name>
```

Name it as a single logical change. One concern per file.

## Governance rule

**No direct schema changes in production. Every schema change requires a migration file.**
