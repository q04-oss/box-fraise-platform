# /scorecard

Read every file across all three crates — domain, server, integrations —
before scoring. Do not estimate or infer. Read the actual code.

For each dimension:
1. List every file read as evidence
2. Quote specific function names, line numbers, or patterns that
   support the score
3. Give the score with a written justification of exactly why it
   is that number and not one higher or one lower
4. List what is working well with file references
5. List what is holding the score back with file references
6. State the single highest-leverage improvement with an estimate
   of how much it would move the score

---

## Dimension 1 — Security (out of 10)

Evidence to read:
- server/src/http/middleware/ — every middleware file
- domain/src/domain/auth/ — service.rs, repository.rs
- domain/src/audit.rs
- domain/src/crypto.rs
- server/src/http/extractors/
- .github/workflows/ci.yml — secret scanner, audit jobs

Score criteria:
- 1-3: No auth, no rate limiting, secrets in plaintext
- 4-5: Basic auth present, significant exploitable gaps
- 6-7: Auth solid, rate limiting present, some gaps remain
- 8-9: Auth excellent, rate limiting layered, audit complete,
       no known exploitable paths
- 10: All of the above plus ZK proofs or equivalent

Evaluate:
- Authentication mechanisms and JWT handling
- Rate limiting implementation and coverage
- Input validation and sanitisation
- Cryptographic primitives and correct use
- HMAC middleware and request signing
- Audit logging — does audit::write() actually land rows
- Any routes that would 500 against the current BFIP schema
- Any silent failure modes

---

## Dimension 2 — Architecture (out of 10)

Evidence to read:
- Cargo.toml (workspace root) — crate structure
- domain/src/lib.rs — public API surface
- server/src/app.rs — middleware ordering
- domain/src/domain/*/routes.rs — layer discipline
- domain/src/domain/*/service.rs — CQRS naming
- domain/src/domain/*/repository.rs — SQL only
- domain/src/events.rs — event bus
- domain/src/error.rs vs server/src/error.rs — error boundary

Score criteria:
- 1-3: Monolith, no separation, everything in handlers
- 4-5: Some separation, significant layer violations
- 6-7: Clean layers, some boundary violations
- 8-9: Enforced boundaries, CQRS, event bus wired
- 10: All of the above plus zero circular dependencies,
     full compiler enforcement

Evaluate:
- Workspace split and crate boundary enforcement
- Layer discipline (routes → service → repository, no skipping)
- Domain event bus — which events are wired, which are dangling
- CQRS naming consistency (get_, list_, claim_, command_)
- Error boundary at HTTP layer (domain never imports axum)
- Schema alignment — which BFIP tables have Rust implementations
- Any circular dependencies or boundary violations

---

## Dimension 3 — Engineer Usability (out of 10)

Evidence to read:
- server/tests/ — every test file, count and categorise
- .github/workflows/ci.yml — every job
- README.md, CONTRIBUTING.md, WORKFLOW.md
- .claude/commands/audit.md — class count and quality
- Justfile — recipe count and coverage
- server/src/domain/*/routes.rs — OpenAPI annotations

Score criteria:
- 1-3: No tests, no CI, no docs
- 4-5: Some tests, basic CI, minimal docs
- 6-7: Good test coverage, CI running, docs exist
- 8-9: Comprehensive tests, full CI, excellent docs,
       new engineer can onboard in under an hour
- 10: All of the above plus property tests, fuzz targets,
     contract tests, and zero setup friction

Evaluate:
- Test coverage by type: unit, service, handler, integration,
  property-based, fuzz, contract
- CI jobs: what each catches, what's missing
- Documentation completeness and accuracy
- Local development setup — what works without Docker/Railway
- OpenAPI spec — how many routes are annotated
- Justfile — what commands exist and what's missing

---

## Dimension 4 — Protocol Conformance (out of 10)

Evidence to read:
- server/migrations/001_bfip_schema.sql — full table list
- domain/src/domain/ — every domain directory
- Compare against BFIP v0.1.2 sections 1-19

Score criteria:
- 1-2: Schema only, no Rust implementation
- 3-4: 1-2 protocol sections implemented
- 5-6: Core auth sections implemented
- 7-8: Presence or attestation implemented
- 9-10: Full protocol implemented including soultokens

Score as:
  (implemented BFIP sections / 19 total sections) * 10

For each BFIP section state:
- Section number and name
- Status: Implemented / Partial / Schema only / Not started
- Which Rust files implement it (if any)

---

## Dimension 5 — Operational Readiness (out of 10)

Evidence to read:
- server/src/main.rs — startup and shutdown handling
- server/src/app.rs — middleware stack
- server/src/http/middleware/ — correlation ID, logging
- .env.example — environment variable completeness
- domain/src/config.rs — startup validation
- Any health check or metrics routes

Score criteria:
- 1-3: No logging, no health check, crashes on bad config
- 4-5: Basic logging, some config validation
- 6-7: Structured logging with correlation IDs, config
       validation at startup, basic health check
- 8-9: All of the above plus graceful shutdown, metrics,
       alerting on critical failures
- 10: All of the above plus runbooks, SLOs, on-call setup

Evaluate:
- Structured logging and correlation IDs on every request
- Health check endpoint existence and what it checks
- Graceful shutdown handling
- Environment variable completeness and fail-fast validation
- Railway deployment configuration
- Any unhandled panic paths

---

## Dimension 6 — Product Completeness (out of 10)

Evidence to read:
- server/src/app.rs — every registered route
- domain/src/domain/*/routes.rs — every handler
- Compare against BFIP v0.1.2 intended user flows

Score criteria:
- 1-2: Auth only
- 3-4: Auth + 1-2 additional flows
- 5-6: Core flows working end to end
- 7-8: Most flows working, some gaps
- 9-10: All intended flows working

Score as:
  (working end-to-end flows / total intended flows) * 10

List every intended user flow and mark each:
- Working end to end
- Partial (some Rust, not complete)
- Schema only
- Not started

---

## Scoring and output

After evaluating all six dimensions:

1. State each score with a one-paragraph written justification
2. State the overall score as the straight average
3. State the weighted score (Security and Protocol Conformance
   weighted 1.5x — they are the most critical for this platform)
4. List the six highest-leverage improvements in priority order
5. Estimate how much each improvement would move the overall score
6. Give a letter grade: A (9+), B (7-8.9), C (5-6.9), D (3-4.9), F (<3)

Append results to SCORECARD.md in this format:

---
## [YYYY-MM-DD HH:MM] Scorecard

| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|---------|
| Security | X/10 | 1.5x | X |
| Architecture | X/10 | 1.0x | X |
| Engineer Usability | X/10 | 1.0x | X |
| Protocol Conformance | X/10 | 1.5x | X |
| Operational Readiness | X/10 | 1.0x | X |
| Product Completeness | X/10 | 1.0x | X |
| **Overall (straight)** | **X/10** | | |
| **Overall (weighted)** | **X/10** | | |
| **Grade** | **X** | | |

### Justifications
[one paragraph per dimension explaining exactly why that score
and not one higher or lower]

### Top 6 improvements
1. [improvement] → estimated +X to overall score
2. [improvement] → estimated +X to overall score
3. [improvement] → estimated +X to overall score
4. [improvement] → estimated +X to overall score
5. [improvement] → estimated +X to overall score
6. [improvement] → estimated +X to overall score

### Summary
[two sentences maximum]
