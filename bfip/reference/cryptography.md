# BFIP Cryptographic Reference

> Status: first edition in this repository, generated as part of Hardening
> Section 1d. Describes the cryptographic primitives as actually implemented
> in `box-fraise-platform`. Where this document deviates from the original
> Hardening 1d spec, the deviations match the live code (see CHANGELOG in
> `bfip/PROTOCOL.md`).

## Section 1 — Beacon UUID PRF

### Construction

```text
input  = business_id.to_string() + ":" + date (ISO 8601, YYYY-MM-DD UTC)
output = HMAC-SHA256(secret_key, input)[0..16] hyphenated as 8-4-4-4-12
```

### Security statement

HMAC-SHA256 is a secure PRF under the PRF assumption on SHA-256 (Bellare et
al. 1996, *Keying Hash Functions for Message Authentication*). Breaking
beacon UUID predictability reduces to breaking HMAC-SHA256 as a PRF.
Security parameter: 128-bit (2^128 adversarial queries required to
distinguish from random).

### Domain separation

The `":"` separator prevents cross-component length-extension confusion
between `business_id` and `date`.

### Daily rotation

Each calendar date produces an independent UUID-shaped value. Past values
give zero information about future values — independence follows from PRF
security.

### UUID format note

The output uses UUID *shape* (8-4-4-4-12 hyphenated hex, 36 chars) but is
**not** an RFC 4122 v4 UUID — the version (byte 6) and variant (byte 8) bits
are not set. The value is a 128-bit PRF output displayed in UUID layout, never
used as an RFC 4122 identifier.

## Section 2 — Beacon witness HMAC

### Construction

```text
input  = business_id.to_string() + ":" + date (YYYY-MM-DD UTC) + ":" + user_id.to_string()
output = HMAC-SHA256(secret_key, input) as 64-char lowercase hex
```

### Security

Inherits 128-bit PRF security from HMAC-SHA256. A user cannot forge a witness
for a beacon they were not near because they do not know `secret_key`.

### Binding

`business_id` binds to a specific beacon, `date` binds to a specific calendar
day (preventing replay across days), and `user_id` binds to a specific user
(preventing one user's witness from being reused by another).

## Section 3 — Display code derivation

### Construction

```text
input        = uuid.as_bytes() (16 raw bytes)
hmac_output  = HMAC-SHA256(SOULTOKEN_HMAC_KEY, input)
base36       = base36_encode(hmac_output[0..9])
display_code = XXXX-XXXX-XXXX (hyphens at positions 4 and 8)
```

### Key rotation

Key version is recorded on each soultoken row
(`soultokens.display_code_key_version`). Rotate by incrementing the version
counter and updating `SOULTOKEN_HMAC_KEY`. Existing display codes remain valid
indefinitely without re-derivation.

Multi-version key lookup is **not yet wired up**: every soultoken is currently
derived with the single configured `SOULTOKEN_HMAC_KEY` (version 1). Full
rotation support is deferred until rotation is operationally required.

## Section 4 — Soultoken signature

### Construction (v0.2.0 — Ed25519)

```text
payload   = uuid + "|" + holder_user_id + "|" +
            issued_at.to_rfc3339() + "|" + expires_at.to_rfc3339() + "|" +
            display_code
hash      = SHA-256(payload)
signature = Ed25519_Sign(SOULTOKEN_SIGNING_KEY, hash)
```

The platform signs `SHA-256(payload)` rather than the raw payload bytes —
matches `Ed25519KeyPair::sign` in `domain/src/crypto/ed25519.rs`.

### Verification

Fetch verifying key from `GET /api/trust-registry/public-key`. Verify with:

```text
Ed25519_Verify(verifying_key, SHA-256(payload), signature)
```

### Trust registry

Public endpoint: `GET /api/trust-registry/public-key`
Returns:
```json
{
  "verifying_key_hex": "<64 hex chars>",
  "algorithm":         "Ed25519",
  "bfip_version":      "0.2.0",
  "description":       "..."
}
```

### Previous implementation (v0.1.x — HMAC-SHA256)

Replaced in v0.2.0. HMAC-SHA256 required the verifier to hold the secret key,
preventing trustless third-party verification. Ed25519 allows anyone with the
public key from the trust registry to verify a soultoken offline.

## Section 5 — Attestation co-signing

### Construction (v0.2.0 — Aggregated Ed25519)

```text
payload = attestation_id + "|" + visit_id + "|" + user_id + "|" +
          photo_hash + "|" + "BFIP_ATTESTATION_V1"
```

`photo_hash` is the empty string when absent. Each signer (delivery staff +
two reviewers) produces an Ed25519 signature over `SHA-256(payload)`.

### Storage format

Each `visit_signatures.signature` row and `visit_attestations.staff_signature`
column stores `verifying_key_hex + ":" + signature_hex`. The verifying key is
co-located with its signature so a third party can re-run aggregated verify
offline without consulting an external reviewer-key directory.

### Verification

All three signatures must verify against the same payload. The two reviewer
signatures additionally pass through aggregated verify before approval:

```text
verify_aggregated_ed25519([vk_1, vk_2], payload, [sig_1, sig_2])
```

### Previous implementation (v0.1.x)

Plain text signature strings with no cryptographic verification. Replaced in
v0.2.0.

## Section 6 — Attestation tokens

### Construction

```text
raw_token  = 32 random bytes from OsRng, hex-encoded (64 chars)
token_hash = SHA-256(raw_token), hex-encoded, stored in DB
```

### Security

The raw token has 256 bits of entropy. The stored `token_hash` is one-way —
the hash alone cannot be replayed; only the original `raw_token` (presented
once at issuance) can later verify against it. Single-use enforcement via
`verified_at` timestamp on the row.
