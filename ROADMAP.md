# Box Fraise Platform — Roadmap

The complete project arc, from the current backend through Web3 settlement.
Phases ship in the order listed; deferred items at the bottom are tracked
but not blocking.

---

## Current state

- **Domains**: 16 (every BFIP v0.2.0 section implemented)
- **Tests**: 390 passing across the workspace
- **Scorecard**: B+ baseline pre-hardening — re-score after Section 12
- **Hardening pass**: complete (12 sections)
- **Protocol**: BFIP v0.2.0, BFMP v0.1.0, BFAP v0.1.0 (stub)

---

## Phase 1 — Server (complete)

The 16 backend domains, with the BFIP section each implements:

| Domain                       | BFIP § |
|------------------------------|--------|
| `auth`                       | 3.1    |
| `users`                      | 13     |
| `identity_credentials`       | 3      |
| `background_checks`          | 3b     |
| `presence`                   | 5      |
| `attestations`               | 6      |
| `attestation_tokens`         | 11     |
| `soultokens`                 | 7      |
| `staff`                      | 6.1, 10 |
| `beacons`                    | 8      |
| `businesses`                 | 12     |
| `orders`                     | 9      |
| `support`                    | 10     |
| `verification_events`        | 14, 17 |
| `platform_configuration`     | 15     |
| `dorotka` (LLM assistant)    | n/a — platform service |

Plus the cross-cutting `analytics` and `notifications` modules added in
Hardening §5 and §7.

---

## Phase 2 — Hardening pass (complete)

| Section | Title                                  | Last commit |
|---------|----------------------------------------|-------------|
| 1       | Cryptographic upgrades (Ed25519, PRF doc, audit) | `ed3885f` |
| 2       | RLS + access control                   | `2ec6be6` |
| 3       | S3 evidence storage (DigitalOcean Spaces) | `04967d4` |
| 4       | Observability (Prometheus, Sentry, health) | `c3848a6` |
| 5       | Internal product analytics             | `d20e8d9` |
| 6       | API hardening (CORS, timeouts, security headers) | `a3f68c2` |
| 7       | SSE real-time notifications            | `107d5f3` |
| 8       | Infrastructure (Dockerfile, nginx, systemd, CI) | `07fd64e` |
| 9       | Data compliance (GDPR / CCPA, retention) | `6caba70` |
| 10      | Operational (feature flags, billing, admin tooling) | `c9af95a` |
| 11      | Protocol updates (BFAP stub, BFIP v0.2.0) | `aea9569` |
| 12      | Documentation                          | this commit |

---

## Phase 3 — Whisked app

The matcha bar's loyalty surface, sharing the same backend.

- Whisked-specific domains on `box-fraise-platform`.
- Whisked business type added to `locations`.
- Whisked Dorotka context (already present — host-header switching).
- Separate iOS app, same backend.

---

## Phase 4 — VPS migration

Migrate off Railway onto a fully self-managed VPS for cost and control.

- DigitalOcean Droplet (Ubuntu 24.04 LTS, 2 GB RAM minimum).
- nginx + certbot SSL (`deploy/nginx.conf`).
- UFW firewall (ports 22, 80, 443 only).
- systemd service (`deploy/box-fraise-platform.service`).
- Prometheus + Grafana + Metabase installed.
- First QPS benchmark against staging.
- `APP_USER_DATABASE_URL` set so RLS enforcement actually fires.

---

## Phase 5 — `box-fraise-terminal`

In-store hardware. Repo: `q04-oss/box-fraise-terminal`.

- Hardware: M5Stack CardputerZero.
- Rust TUI for the built-in display (staff terminal).
- Web dashboard for HDMI monitor (staff-facing).
- Beacon daemon (BLE broadcasting, daily UUID rotation).
- NFC daemon (box tap detection and validation).
- Mesh daemon (peer relationships, offline event queue).
- Mode switching: business / staff / user via soultoken tap.
- Delegation token: business device issues to staff device.

---

## Phase 6 — iOS full integration

The full BFIP protocol on the device, not just the API surface.

- Identity verification (Stripe Identity).
- Cooling-period app-open recording.
- Presence session detection (CoreBluetooth).
- NFC box tap.
- Attestation flow.
- Soultoken display and management.
- Background check submission.
- BFMP on device (MultipeerConnectivity, UWB).
- Signal Protocol messaging (cleared channel only).
- USDC wallet integration.

---

## Phase 7 — BFAP implementation

Repo: `q04-oss/bfap` (stub spec lives in this repo at `bfap/PROTOCOL.md`).

- Agent hardware identity (HSM / TPM / Secure Enclave).
- Capability certificates with formal semantics.
- Cryptographic hash-chain provenance log.
- Three-tier behavioural attestation.
- Peer attestation network.
- Adversarial scenario library.
- Agent soultoken issuance.

---

## Phase 8 — Web3

Repo: `q04-oss/box-fraise-contracts`. Optimism deployment.

- `BFIPRegistry.sol` — human soultoken registry.
- `DelegationRegistry.sol` — staff authority delegation.
- `TapSettlement.sol` — USDC payment with 3% platform fee.
- `EncounterRegistry.sol` — verified physical encounters.
- `BFAPRegistry.sol` — agent credential registry.

---

## Phase 9 — Scale scaffolding

Document thresholds — implement when triggered, not before.

| Lever                | Threshold                                                      |
|----------------------|----------------------------------------------------------------|
| Kafka                | message-queue depth > 1 000 / min sustained 7 days             |
| Database sharding    | `presence_events` > 100 M rows                                 |
| Kubernetes           | VPS CPU > 70% sustained, vertical scaling exhausted            |
| Multi-region         | users in 3+ geographic regions                                 |
| CDN                  | static-asset bandwidth > 1 TB / month                          |
| Read replicas        | analytics queries affecting API latency                        |

---

## Phase 10 — Formal cryptographic hardening

The "if-we-survive-and-grow" pass. Most of this is academic-grade.

- ZK proof of presence (replace HMAC beacon witness).
- Threshold Schnorr signatures (replace aggregated Ed25519).
- W3C Verifiable Credential JSON population.
- Full formal security proofs in `bfip` and `bfap` repos.
- Independent cryptographic audit.

---

## Deferred items (tracked)

Items the hardening pass identified but didn't ship in-section. Each has a
TODO marker in code or docs at the relevant call site.

- **RLS §2d**: per-request transaction refactor (RLS policies are inert until this lands).
- **§3 evidence hash enforcement**: the client-supplied `evidence_hash` is still trusted; the server-computed hash from the upload endpoint is canonical but not yet enforced at the `complete_visit` call site.
- **§9 record_consent → background-check initiation**: function exists, call site is TODO.
- **§7 SSE event payloads**: `display_code` in `SoultokenIssued`, `business_id` in `OrderReady`.
- **§11 `constant_time_eq` utility crate**: would let `integrations` import the canonical impl instead of duplicating.
- **§1c `SOULTOKEN_HMAC_KEY` multi-version key rotation**: the version field is recorded; multi-key lookup deferred until rotation is operationally required.
- **§10 Stripe billing webhook**: the `business_subscriptions` table is scaffolding; row creation/updates ship with the iOS app.
- ~~**§6 per-user rate-limit middleware**: limit values seeded in `platform_configuration` (migration 009); per-user keying deferred pending post-auth middleware.~~ — **shipped Grade A item 3**: see `server/src/http/middleware/user_rate_limit.rs`. Wired into `attestations`, `background_checks`, `identity`, `dorotka` initiate routes; reads from `platform_configuration` so ops retune without redeploy.
