# box-fraise-platform Scorecard
Track quality over time. Run with: claude /scorecard

---
## [2026-05-01] Scorecard

| Dimension | Score |
|-----------|-------|
| Security | 6.5 / 10 |
| Architecture | 7 / 10 |
| Engineer Usability | 7 / 10 |
| Protocol Conformance | 2 / 10 |
| Operational Readiness | 5 / 10 |
| Product Completeness | 3 / 10 |
| **Overall** | **5.2 / 10** |

**Highest-leverage improvements:**
1. **Security** — Fix `audit.rs` INSERT to match BFIP schema: drop `business_id` and `ip_address`, add `user_id`. Every audit write currently fails silently.
2. **Architecture** — Remove dead `KeyId`/`MessageId` exports from `types/mod.rs`; move `audit::write` in dorotka route into a service function to restore layer discipline.
3. **Engineer Usability** — Add `.env.example`, create `WORKFLOW.md`, replace `eprintln!("skipping")` test pattern with `#[ignore = "requires REDIS_URL"]`.
4. **Protocol Conformance** — Wire `magic_link_tokens` and `jwt_revocations` DB tables into existing auth code (additive alongside Redis writes) to complete Section 1.
5. **Operational Readiness** — Add graceful shutdown: `axum::serve(...).with_graceful_shutdown(shutdown_signal())` — one function, eliminates dropped in-flight requests on Railway redeploy.
6. **Product Completeness** — Implement `POST /api/businesses` registration endpoint — unlocks the entire downstream chain (beacons → presence → attestation → soultokens).

**Summary:** Production-grade auth and middleware foundation with clean architecture, but the audit trail is silently broken against the BFIP schema, BFIP sections 2–10 are schema-only, and the platform lacks graceful shutdown and observability.

---
## [2026-05-01 v2] Scorecard

| Dimension | Score | Δ |
|-----------|-------|---|
| Security | 7.5 / 10 | +1.0 |
| Architecture | 7 / 10 | — |
| Engineer Usability | 7.5 / 10 | +0.5 |
| Protocol Conformance | 2 / 10 | — |
| Operational Readiness | 5.5 / 10 | +0.5 |
| Product Completeness | 3 / 10 | — |
| **Overall** | **5.4 / 10** | **+0.2** |

**Change since last scorecard:** Fixed silent audit trail failure — `audit.rs` INSERT now targets correct BFIP columns `(event_kind, user_id, actor_id, metadata)`. Two `sqlx::test` tests confirm rows land in the database. All 5 call sites updated; `ip_address` preserved in `metadata` JSON.

**Highest-leverage improvements:**
1. **Security** — Validate `FRAISE_HMAC_SHARED_KEY` at startup in `config.rs` (same pattern as `jwt_secret` length check) so misconfiguration fails at boot, not at first iOS request.
2. **Architecture** — Move `audit::write` in `dorotka/routes.rs` into `domain/src/domain/dorotka/service.rs::ask()` — restores routes → service layer discipline and makes the service independently testable.
3. **Engineer Usability** — Generate `.env.example` from `config.rs` `require()`/`optional()` calls; create `WORKFLOW.md`; replace `eprintln!("skipping")` with `#[ignore = "requires REDIS_URL"]`.
4. **Protocol Conformance** — Wire `magic_link_tokens` DB writes into `service::request_magic_link` (additive alongside Redis) to complete BFIP Section 3.1.
5. **Operational Readiness** — Add graceful shutdown: `axum::serve(...).with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); })` — eliminates dropped in-flight requests on Railway redeploy.
6. **Product Completeness** — Implement `POST /api/businesses` registration — unlocks the downstream BFIP chain (beacons → presence → attestation → soultokens).

**Summary:** Audit trail restored after BFIP schema fix; the platform now records all security events for the first time since migration, but architecture, protocol, and product dimensions are unchanged and represent the bulk of remaining work.

---
## [2026-05-01 18:00] Scorecard

| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|---------|
| Security | 7.5/10 | 1.5x | 11.25 |
| Architecture | 7/10 | 1.0x | 7.0 |
| Engineer Usability | 7.5/10 | 1.0x | 7.5 |
| Protocol Conformance | 3/10 | 1.5x | 4.5 |
| Operational Readiness | 5.5/10 | 1.0x | 5.5 |
| Product Completeness | 3/10 | 1.0x | 3.0 |
| **Overall (straight)** | **5.6/10** | | |
| **Overall (weighted)** | **5.54/10** | | |
| **Grade** | **C** | | |

### Justifications

**Security 7.5:** JWT rotation window works (`verify_token` tries current then previous secret); HMAC middleware has constant-time comparison, nonce dedup, 5-min window; rate limiting is dual-backend; audit trail now writes correctly to BFIP schema with immutable DB trigger. Stops at 7.5 because App Attest assertion verification is explicitly deferred in `hmac.rs` ("phase 2") and `FRAISE_HMAC_SHARED_KEY` is optional with no startup warning — an unconfigured server silently 500s iOS requests.

**Architecture 7:** Domain crate compiles without axum; `From<DomainError> for AppError` is exhaustive; CQRS naming is consistent across all 5 service functions; three-crate workspace with enforced boundaries. Stops at 7 because `dorotka/routes.rs:69` calls `audit::write` directly (layer violation), `KeyId`/`MessageId` dead exports remain in `types/mod.rs`, and `staff.rs` is a fully implemented dead branch.

**Engineer Usability 7.5:** 78 test functions spanning unit, sqlx::test, handler, integration, proptest (8 tests), fuzz (2 targets), and compile-time contracts; 7 CI jobs; WORKFLOW.md is substantive (4-phase, test-first). Stops at 7.5 because `.env.example` does not exist, `server/tests/auth.rs` is a 0-test stub, and the OpenAPI spec is hand-built with no handler annotations — it can drift silently.

**Protocol Conformance 3:** Apple Sign In verification, magic link Redis flow, and JWT issuance/revocation work end-to-end — that's roughly 3 of 19 BFIP sections partially implemented. `magic_link_tokens`, `apple_auth_sessions`, and `jwt_revocations` tables are never written to. Sections 4–19 (identity verification, cooling period, presence, soultokens, beacons, businesses, attestation, orders, support) are all schema-only with zero Rust implementation.

**Operational Readiness 5.5:** Structured logging with correlation IDs (`X-Request-Id` on every response, spans with `request_id`, `method`, `path`, `status`, `latency_ms`), health check at `/health` exercises both DB and Redis, config fails fast with actionable messages. Stops at 5.5 because `lib.rs:75` has no `.with_graceful_shutdown()` — every Railway redeploy drops in-flight requests — and there are no metrics (no Prometheus, no OpenTelemetry).

**Product Completeness 3:** 9 flows work end-to-end (Apple auth, magic link, profile CRUD, user search, Dorotka AI). 18 of 27 intended BFIP flows are entirely absent. The working flows are all in the auth/AI surface; nothing in identity verification, business operations, or commerce is reachable by a real user.

### Top 6 improvements
1. Graceful shutdown in `lib.rs` (one line) → Operational +1.0, **+0.17 overall**
2. Implement `POST /api/businesses` → Product +0.5 + Protocol +0.5, **+0.21 weighted**
3. Wire `magic_link_tokens` DB writes alongside Redis in `service::request_magic_link` → Protocol +0.5, **+0.11 weighted**
4. Generate `.env.example` from `config.rs` → Usability +0.5, **+0.08 overall**
5. Move dorotka `audit::write` into `service::ask` → Architecture +0.5, **+0.08 overall**
6. Validate `FRAISE_HMAC_SHARED_KEY` required at startup → Security +0.3, **+0.05 overall**

### Summary
The auth and middleware foundation is production-quality with genuine crypto depth, but Protocol Conformance and Product Completeness both score 3/10 because 18 of 27 intended user flows and 16 of 19 BFIP sections have no Rust implementation — the weighted score (5.54) is anchored by the 1.5x Protocol Conformance penalty, and implementing business registration would produce the largest single-session movement.
