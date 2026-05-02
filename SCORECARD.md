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
