Read every file across all three crates — domain, server, integrations —
before scoring. Do not estimate. Read the actual code.

Evaluate box-fraise-platform across six dimensions.
For each dimension give:
- Score out of 10
- What is working well (specific file references)
- What is holding the score back (specific file references)
- The single highest-leverage improvement

## 1. Security
- Authentication and JWT handling
- Rate limiting implementation and coverage
- Input validation and sanitisation
- Cryptographic primitives and correct use
- HMAC middleware and request signing
- Audit logging coverage
- Any routes that would 500 at runtime against the BFIP schema
- Any silent failure modes remaining

## 2. Architecture
- Workspace split and crate boundary enforcement
- Layer discipline (routes → service → repository)
- Domain event bus wiring
- CQRS naming consistency
- Error boundary at HTTP layer
- Schema alignment with BFIP v0.1.2
- Any circular dependencies or boundary violations

## 3. Engineer Usability
- Test coverage: unit, service, handler, integration, property-based
- CI job coverage
- Documentation: README, CONTRIBUTING, WORKFLOW, audit command
- Local development setup
- OpenAPI spec coverage
- Justfile completeness

## 4. Protocol Conformance
- How much of BFIP v0.1.2 is implemented versus schema-only
- Which sections have working Rust code
- Which tables are waiting to be built
- Score as: implemented sections / total sections

## 5. Operational Readiness
- Structured logging and correlation IDs
- Health check endpoints
- Graceful shutdown
- Environment variable completeness and validation
- Railway deployment configuration
- Alerting on critical failures

## 6. Product Completeness
- What can a real user actually do right now
- Which user-facing flows are end-to-end functional
- Which flows exist in schema only
- Score as: working flows / total intended flows

Overall score (average of six) and one-sentence summary.

After scoring, append the results to SCORECARD.md at the repo root
in this format:

---
## [YYYY-MM-DD] Scorecard

| Dimension | Score |
|-----------|-------|
| Security | X / 10 |
| Architecture | X / 10 |
| Engineer Usability | X / 10 |
| Protocol Conformance | X / 10 |
| Operational Readiness | X / 10 |
| Product Completeness | X / 10 |
| **Overall** | **X / 10** |

**Highest-leverage improvements:**
1. [Security finding]
2. [Architecture finding]
3. [Engineer Usability finding]
4. [Protocol Conformance finding]
5. [Operational Readiness finding]
6. [Product Completeness finding]

**Summary:** [one sentence]

If SCORECARD.md does not exist, create it with a header:
# box-fraise-platform Scorecard
Track quality over time. Run with: claude /scorecard
