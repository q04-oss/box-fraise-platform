# Migrations

## Structure

| File | What it establishes |
|---|---|
| `000_initial_schema.sql` | Full baseline schema captured from Railway via pg_dump. **Do not split or rename** — it is checksum-tracked in `_sqlx_migrations` on the live database. |
| `001_add_public_key_to_device_attestations.sql` | EC public key column on `device_attestations` for App Attest assertion verification. |
| `002_create_attest_challenges.sql` | `attest_challenges` table — server-issued single-use challenges to bind App Attest attestations. |
| `003_square_oauth_and_staff.sql` | Square OAuth token storage, staff location assignments, audit_events table. |
| `004_loyalty.sql` | `business_loyalty_config` and `loyalty_events` tables, indexes. |
| `005_venue_drinks_and_orders.sql` | `venue_drinks` and `venue_orders` tables for in-venue drink ordering. |
| `006_nfc_stickers.sql` | `nfc_stickers` table for NFC cup sticker loyalty redemption. |
| `007_fix_check_constraints.sql` | Corrects check constraint definitions that were too strict on enum columns. |
| `008_business_id_on_devices_and_orders.sql` | Adds `business_id` directly to `devices` and `orders` — removes JOIN workarounds for business scope enforcement. |

## Domain groupings within 000

`000_initial_schema.sql` is a pg_dump and therefore alphabetical, not domain-ordered. Search for `-- === DOMAIN:` to jump between logical sections:

- **TYPES** — ENUM types (`order_status`, `membership_tier`, `device_role`, etc.)
- **AUTH** — `users`, password reset tokens, magic link state
- **DEVICES** — `device_attestations`, `device_pairing_tokens`, `devices`
- **BUSINESSES** — `businesses`, `locations`, `employment_contracts`, `business_accounts`
- **ORDERS** — `orders`, `catalog_varieties`, `batches`, `time_slots`, `bundle_*`
- **LOYALTY** — `loyalty_events` base tables (extended in 004 and 006)
- **SOCIAL** — `connections`, `collectifs`, `campaigns`, `community_*`, `evening_tokens`
- **ART** — `artworks`, `drops`, `art_*`, `portrait_tokens`
- **FINANCIAL** — `earnings_ledger`, `credit_transactions`, `memberships`, `membership_funds`
- **MESSAGING** — `conversation_archives`, `platform_messages`
- **CONSTRAINTS** — all `ALTER TABLE ADD CONSTRAINT` / `CREATE INDEX` statements

## Running migrations locally

```bash
cd server
sqlx migrate run --database-url "$DATABASE_URL"
```

For a fresh local database:

```bash
createdb fraise_dev
DATABASE_URL=postgres://localhost/fraise_dev sqlx migrate run
```

## Adding a new migration

```bash
sqlx migrate add --source migrations <descriptive_name>
```

Name it as a single logical change: `add_referral_codes`, `add_table_bookings_to_users`, `create_waitlist_table`. One concern per file.

## Governance rule

**No direct schema changes in production. Every schema change requires a migration file.** This rule exists because the schema and code have drifted three times — `tournaments`/`earnings_ledger` (tables referenced in code but never in migrations), `popup_events`/`event_id` (webhook handler referencing non-existent table and column), and `popup_rsvps.popup_id` (column renamed in production but not reflected in migrations) — each time requiring a session to diagnose and fix.
