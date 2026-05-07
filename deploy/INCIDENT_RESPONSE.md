# Box Fraise Platform — Incident Response Runbook

Hardening §10. The on-call playbook. Everything here assumes you have shell
access to the VPS (or Railway dashboard) and credentials for Sentry, the
PostgreSQL database, and the metrics dashboard.

---

## Severity levels

| Level | Definition                                                       | Examples                                                                            |
|-------|------------------------------------------------------------------|-------------------------------------------------------------------------------------|
| P0    | Platform completely down                                         | Database unreachable; server crashed; all `/health` requests return 5xx.            |
| P1    | Core feature broken                                              | Attestation flow rejecting all submissions; soultokens not issuing; auth failing.   |
| P2    | Degraded performance                                             | p99 latency > 2s; error rate > 1%; partial regional outage.                         |
| P3    | Single-user or cosmetic bug                                      | One user reports their soultoken display code has a typo (it doesn't, but…).        |

---

## P0 response

1. **Confirm the symptom.** Hit `/health` — does it 503? Curl from a second network so you're not chasing your own problem.
   ```sh
   curl -sS https://api.boxfraise.com/health
   ```
2. **Check the platform.** Railway / DigitalOcean console — is the host up, is Postgres up?
3. **Check logs.**
   ```sh
   journalctl -u box-fraise-platform -n 200 --no-pager
   ```
4. **Check Sentry** for recent error-level events. Stack traces will tell you whether it's the application or infrastructure.
5. **Restart if it's the server:**
   ```sh
   systemctl restart box-fraise-platform
   ```
6. **Roll back if restart doesn't help.** The previous binary should be at `/opt/box-fraise-platform/server.previous` (per `deploy/DEPLOY.md`). Swap it back, restart, retry `/health`.
7. **If Postgres is the problem,** contact Railway support or check the DigitalOcean managed-database dashboard before doing anything destructive locally.

---

## P1 response

1. **Identify the affected route** via Prometheus — sort `http_requests_total{status=~"5.."}` by route.
2. **Read the Sentry stack trace.** Is the failure in service code (logic bug) or DB (data corruption)?
3. **If logic:** revert the offending commit, hotfix-deploy.
4. **If data:** DO NOT modify any rows yet. Snapshot the current state for the post-incident report, then escalate to whoever wrote the affected domain.

---

## P2 response

1. Check `/metrics` for in-flight requests, latency histogram, error rate.
2. Common causes:
   - Postgres connection pool saturated → bump `DB_MAX_CONNECTIONS`.
   - Redis unavailable (rate-limit middleware falls back to in-process — single-instance only at that point).
   - Anthropic / Stripe / Resend upstream slow.
3. If a single dependency is at fault, the symptom usually is that one route is slow and the rest are fine.

---

## P3 response

File a ticket. Don't page anyone. If it's a single user with a soultoken
issue, check the `verification_events` table for that user before
assuming bug — most "missing display code" reports turn out to be
unrelated (cached, wrong account, etc.).

---

## Escalation timing

| Level | Page on-call within |
|-------|---------------------|
| P0    | immediately         |
| P1    | within 1 hour       |
| P2    | within 4 hours      |
| P3    | next business day   |

---

## Post-incident

Within 24 hours of resolution:

1. Write an incident report covering: timeline, customer impact (count of affected requests / users), root cause, the fix.
2. Add a regression test for the failure mode if possible. If not possible (e.g. third-party outage), add monitoring or a feature flag.
3. Update this runbook if the response steps were wrong or missing.
4. Open a post-mortem item in the engineering tracker if the cause is structural (bad architecture, missing observability) rather than tactical.
