-- Add DER-encoded public key column to device_attestations.
-- Populated at attestation time; used by HMAC middleware to verify
-- per-request App Attest assertions (ECDSA-P256-SHA256).
ALTER TABLE device_attestations
    ADD COLUMN IF NOT EXISTS public_key TEXT;
