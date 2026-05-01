#!/usr/bin/env bash
# check_schema_drift.sh — Migration integrity and schema drift detection.
#
# Applies all migrations to the database pointed at by DATABASE_URL, then
# cross-references the resulting table list against table names referenced in
# Rust source. Fails with a clear error if any referenced table is absent from
# the migrated schema.
#
# Ignored: _sqlx_migrations (sqlx internal tracking table).
#
# Requires:
#   DATABASE_URL  — pointing at a writable Postgres database
#   sqlx-cli      — cargo install sqlx-cli --no-default-features --features postgres
#
# Usage:
#   DATABASE_URL=postgres://fraise:fraise@localhost/fraise_drift \
#     bash scripts/check_schema_drift.sh

set -euo pipefail

MIGRATIONS_DIR="server/migrations"

# ── Step 1: Apply all migrations ──────────────────────────────────────────────
echo "==> Applying migrations..."
sqlx migrate run --source "$MIGRATIONS_DIR"
echo "    Done."

# ── Step 2: Extract table names from the live schema ─────────────────────────
echo "==> Reading schema table list from Postgres..."
SCHEMA_TABLES=$(psql "$DATABASE_URL" -t -A -c \
    "SELECT table_name
       FROM information_schema.tables
      WHERE table_schema = 'public'
        AND table_type   = 'BASE TABLE'
        AND table_name  <> '_sqlx_migrations'
      ORDER BY table_name;")

echo "    Schema tables:"
echo "$SCHEMA_TABLES" | sed 's/^/      /'

# ── Step 3: Extract table names referenced in Rust source ────────────────────
echo "==> Scanning Rust source for SQL table references..."

# Grep for identifiers that immediately follow FROM / JOIN / INTO / UPDATE in
# raw SQL string literals within .rs files. The pattern handles both bare names
# and double-quoted names, and is intentionally conservative to avoid false positives.
RS_TABLES=$(grep -roh --include="*.rs" \
    -P '(?i)(?:FROM|JOIN|INTO|UPDATE)\s+"?([a-z][a-z0-9_]{1,50})"?' \
    domain/src/ server/src/ integrations/src/ 2>/dev/null \
    | grep -oP '[a-z][a-z0-9_]+$' \
    | sort -u \
    | grep -vxF -e select -e where -e set -e and -e or -e not \
                -e null -e true -e false -e on -e is -e in \
                -e by -e as -e to -e if -e do -e for -e at \
    || true)

echo "    Referenced tables:"
echo "$RS_TABLES" | sed 's/^/      /'

# ── Step 4: Compare ──────────────────────────────────────────────────────────
echo "==> Checking for drift..."
FAILED=0

for table in $RS_TABLES; do
    if ! printf '%s\n' "$SCHEMA_TABLES" | grep -qxF "$table"; then
        echo "    DRIFT: '$table' referenced in source but absent from migrated schema"
        FAILED=1
    fi
done

if [ "$FAILED" -ne 0 ]; then
    echo ""
    echo "ERROR: Schema drift detected."
    echo "       Either the referenced table was dropped in a migration and the"
    echo "       source still references it, or a migration was never written."
    exit 1
fi

echo "==> No schema drift detected. ✓"
