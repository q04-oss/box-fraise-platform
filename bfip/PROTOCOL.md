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
- Section 22 (new): Agent Delegation Credentials stub — full specification
  in `q04-oss/bfap` (forthcoming).
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

This file currently carries only the changelog, the version header, and
the Section 22 stub below. The authoritative full-text protocol lives in
the protocol repository; the cryptographic-primitives appendix is mirrored
here at `bfip/reference/cryptography.md` so the implementation can be
reviewed against a formal description without leaving this codebase.

## Section 22 — Agent Delegation Credentials (BFAP Stub)

This section is reserved for BFAP v0.1.0.

BFIP-attested humans may issue signed delegation credentials to AI agents
authorising them to act on the human's behalf within defined capability
constraints.

Agent credentials depend on:

- A valid BFIP soultoken held by the issuing human.
- Hardware-bound agent identity (HSM / TPM / Secure Enclave).
- Capability certificate with formal semantics.
- Cryptographic hash chain provenance log.

Full specification: see `q04-oss/bfap` (forthcoming).

Implementation status: reserved — not yet implemented.
Target version: BFAP v0.1.0.
