Run `git diff --staged` to get the staged diff. If the diff is empty, run `git diff HEAD~1` to audit the last commit instead. Read every changed file in full — do not work from the diff alone.

Audit the staged changes against the following classes:

1. **Auth missing** — Are there any new route handlers without a `RequireUser`, `RequireDevice`, `RequireStaff`, or `RequireClaims` extractor? Public endpoints must be explicitly intentional; flag any that look like they should be gated.

2. **Client-controlled input** — Does any new code trust client-supplied values without server-side validation? Look for: enum/type fields taken directly from the request body and passed to a database or external API without an allowlist check; user-controlled IDs used without an ownership assertion in the query; metadata fields set from request body that affect downstream logic (e.g. payment type, tier, context).

3. **Fail-open fallbacks** — Are there new Redis or external service calls that return `true` / `Ok(())` / skip the check when the call fails? Any new rate limiter, nonce check, or token store that degrades to "allow" on error must be flagged. The pattern to catch: `Err(_) => return true` or `unwrap_or(())` on a security-critical path.

4. **Assumed external schema** — Does any new handler destructure a response from Stripe, Anthropic, Square, Resend, or any other external API using `.unwrap()`, array indexing, or field access without a `None` / missing-field guard? Silent failures on schema changes are the bug; the fix is always to check before using.

5. **Known-pattern recurrence** — Check the new code against every bug class already patched in this codebase:
   - `trim_start_matches` used where `strip_prefix` is correct (greedy vs single-occurrence stripping)
   - `context` or similar classification fields derived from client input rather than server-side signals (Host header, authenticated session, path)
   - Fixed-window rate limiters where a burst at the window boundary doubles the effective limit
   - `claimed_at IS NULL` or equivalent single-use token pattern missing from a new claim endpoint
   - `constant_time_eq` implemented with an early-return length check instead of HMAC normalisation
   - Business/ownership scope missing from a device or staff action (e.g. collecting or modifying resources across business boundaries)
   - Email or identifier accepted from request body for a verification step that should use the server's own record

6. **Secrets or keys in plain `String` fields** — Does any new config struct field, request body, or database column store a secret as a plain `String` instead of `SecretString`? Look for: API keys, tokens, passwords, private key material stored or logged without protection.

7. **Layer violations** — Does any route handler import `repository` or call a repository function directly? Does any `service.rs` file import `axum`, `Json`, `Router`, or any HTTP type? The rule is strict: `routes.rs → service.rs → repository.rs`, no layer may skip another.

8. **Error boundary** — Does any file in `domain/` or `integrations/` import `axum`, `http::StatusCode`, or `tower`? Does any domain file construct `AppError` or reference HTTP status codes? Domain code must only use `DomainError`. `AppError` and status codes belong exclusively in `server/src/error.rs`.

For each finding, report:
- File and line number
- Which class (1–8) it falls under
- Whether it is exploitable now, needs a precondition, or is theoretical
- The exact attack sequence if exploitable now

List everything before touching anything. After the full list is confirmed, apply surgical fixes only to what was found — no refactoring, no cleanup beyond the identified issues.
