-- =============================================================
-- Grade A item 1 — App Attest per-device public key storage.
--
-- Until this migration `enforce_assertion` was a presence-only stub:
-- it required the `X-App-Attest-Assertion` + `X-App-Attest-Key-Id`
-- headers to be present, but never invoked the cryptographic
-- `verify_assertion` routine because there was nowhere to look up
-- the registered public key.
--
-- These two columns let `register_app_attest_key` persist the
-- DER-SPKI extracted by `parse_attestation` at attestation time, and
-- let `get_app_attest_public_key` look it up by key_id on every
-- request from the device. The partial index keeps the lookup cheap
-- without indexing the (mostly NULL) column for unattested users.
--
-- Idempotent: re-applying is a no-op once the columns and index exist.
-- =============================================================

ALTER TABLE identity_credentials
    ADD COLUMN IF NOT EXISTS app_attest_key_id         TEXT,
    ADD COLUMN IF NOT EXISTS app_attest_public_key_der BYTEA;

CREATE INDEX IF NOT EXISTS idx_identity_credentials_app_attest_key_id
    ON identity_credentials (app_attest_key_id)
    WHERE app_attest_key_id IS NOT NULL;
