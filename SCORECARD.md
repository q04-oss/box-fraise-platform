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
