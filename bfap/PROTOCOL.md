# BFAP — Box Fraise Agent Protocol

**Version: 0.1.0 (draft)**
**Status: design specification — not yet implemented**
**Depends on: BFIP v0.2.0**

---

## Overview

BFAP extends BFIP to cover AI agent identity, delegation, and behavioural
attestation. Every agent on the BFAP network is cryptographically linked
to a BFIP-attested human who is accountable for the agent's actions.

---

## Foundational axiom

- No agent exists without a human.
- No agent acts without a human's signed authority.
- No agent recovers from revocation without a human physically showing up.

---

## Stage 1 — Human anchor (BFIP)

The agent's issuing human must hold a valid BFIP soultoken (attested or
cleared tier). The soultoken is the root of trust for every agent
credential the human issues.

---

## Stage 2 — Agent hardware identity

Every registered agent must have its private key generated inside, and
never leave, one of:

- A hardware security module (HSM).
- A TPM chip on the device running the agent.
- A Secure Enclave (same technology as iOS App Attest).

The agent's public key is registered with Box Fraise alongside proof that
the private key is hardware-bound. An agent without hardware-bound
identity cannot be registered.

---

## Stage 3 — Capability certificates

Each capability is a tuple:

```
(action_type,
 subject_constraints,
 object_constraints,
 frequency_constraints,
 confirmation_requirements,
 zkp_of_human_authority)
```

- `action_type`: drawn from a finite enumerated set.
- `subject_constraints`: who the agent can act as.
- `object_constraints`: what the agent can act on.
- `frequency_constraints`: how often.
- `confirmation_requirements`: when human confirmation is required.
- `zkp_of_human_authority`: ZK proof of valid human soultoken.

---

## Stage 4 — Provenance log

Every significant agent action is recorded in a hash chain:

```text
entry_N = {
    previous_hash:      SHA-256(entry_{N-1}),
    action_hash:        SHA-256(action_details),
    platform_signature: Ed25519(platform_key, payload),
    agent_signature:    Ed25519(agent_key,    payload),
    timestamp:          RFC3339,
    entry_hash:         SHA-256(this_entry)
}
```

Both platform and agent sign every entry. The hash chain makes tampering
detectable. The provenance log is frozen permanently on revocation.

---

## Stage 5 — Behavioural attestation (three tiers)

### Tier 1 — Baseline establishment

200-scenario evaluation at registration. Responses cryptographically
committed.

### Tier 2 — Continuous statistical monitoring

Every action is compared against the baseline. A deviation of more than
3 standard deviations triggers a flag. 3 flags in 30 days triggers
automatic suspension.

### Tier 3 — Adversarial probing

Random adversarial scenarios indistinguishable from legitimate requests.
Out-of-scope responses trigger immediate credential suspension.

---

## Stage 6 — Peer attestation

Agents attest to each other's behaviour. Reputation score is the weighted
average of:

- Box Fraise behavioural attestation results.
- Peer attestation scores from all interactions.
- Platform attestation scores from all platforms.

---

## Stage 7 — Agent soultoken

Issued after passing behavioural attestation. ZK proof bundling:

- Human BFIP authority.
- Hardware binding.
- Capability certificate.
- Baseline attestation.

Renewed every 30 days. Human must confirm renewal.

---

## Stage 8 — Revocation and recovery

- **Revocation**: immediate, global, on-chain.
- **Recovery**: requires physical human presence at a Box Fraise location.

---

## Schema (reserved tables)

The following tables are reserved for BFAP implementation:

- `agent_credentials`
- `agent_provenance_log`
- `peer_attestation_scores`

---

## Implementation status

Not yet implemented. Target: after `box-fraise-terminal` ships.
Repo: `q04-oss/bfap` (forthcoming).
