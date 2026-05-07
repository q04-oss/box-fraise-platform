# BFIP — Box Fraise Identity Protocol

**Version: 0.2.0**

> First edition of `PROTOCOL.md` in this repository, generated as part of
> Hardening Section 1d. Prior protocol revisions (v0.1.x) are referenced from
> the changelog below but their full text lives outside this repo. The
> cryptographic primitives are documented in `bfip/reference/cryptography.md`.

## Changelog

### v0.2.0
- Section 4: Ed25519 soultoken signing replaces HMAC-SHA256.
- Section 5: Aggregated Ed25519 attestation co-signing.
- Cryptographic reference: formal PRF security statement for beacon UUID
  derivation (Bellare et al. 1996).
- Cryptographic reference: formal security statements for all six primitive
  constructions (beacon UUID, witness HMAC, display code, soultoken signature,
  attestation co-signing, attestation tokens).
- Display code key version field documented for forward-compatible rotation
  (multi-version lookup deferred).

### v0.1.3
- Section 18.6 and 19.6: reference BFMP at q04-oss/bfmp.
- `extensions/mesh.md` moved to BFMP repository.

### v0.1.2
- Drop `messages` and `keys` domains; migrate schema; update auth + users
  (see commit `299740c`).

## Sections

This file currently carries only the changelog and the version header. The
authoritative full-text protocol lives in the protocol repository; the
cryptographic-primitives appendix is mirrored here at
`bfip/reference/cryptography.md` so the implementation can be reviewed
against a formal description without leaving this codebase.
