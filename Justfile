# box-fraise-platform Justfile
# Install just: cargo install just
# Usage: just <recipe>

# List available recipes
default:
    @just --list

# Run all tests
test:
    cargo test --workspace

# Static analysis (check + clippy)
check:
    cargo check --workspace
    cargo clippy --workspace -- -D warnings

# Security audit
audit:
    cargo audit
    cargo deny check --manifest-path server/Cargo.toml

# Full local CI pass (check → test → audit)
ci: check test audit

# Check for schema drift between migrations and source references
drift:
    bash scripts/check_schema_drift.sh

# Generate and open documentation
docs:
    cargo doc --no-deps --open

# Run HMAC fuzz target (requires nightly)
fuzz-hmac:
    cargo +nightly fuzz run hmac_verify

# Run sanitiser fuzz target (requires nightly)
fuzz-sanitise:
    cargo +nightly fuzz run sanitise
