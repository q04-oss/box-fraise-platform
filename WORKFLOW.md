# Development Workflow

> See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution rules and [server/migrations/README.md](server/migrations/README.md) for schema change rules.

`/grill-me` is the design gate. `/scar-tissue` is the merge gate. No feature skips both.

---

## Phase 1 -- Design

Before writing any implementation code:

1. Run `/grill-me` with a plain-language description of the feature. Address every finding before moving on.
2. Write handler-level tests first -- three per endpoint, minimum:
   - Happy path
   - Auth failure (missing or invalid token)
   - One failure mode specific to this endpoint (bad input, conflict, external dependency down, etc.)
3. Tests go in `server/tests/`. Use the existing test files as a model; add a new file for a new domain.
4. Run `cargo test` and confirm the new tests **fail**. A test that passes before the feature exists is not a test.

---

## Phase 2 -- Build

1. Implement the feature.
2. After each significant change, run `cargo test` against the Railway test environment.
3. All pre-existing tests must continue to pass. A regression is a blocker, not a to-do.

---

## Phase 3 -- Audit

Before opening a PR:

1. Run `/scar-tissue` on the branch.
2. Fix every confirmed finding. Disputed findings must be documented inline with a rationale.
3. No merge without a clean audit pass.

---

## Phase 4 -- Production Incidents

When something breaks in production:

1. Write a test that would have caught the failure **before** writing the fix.
2. Confirm the test fails against the pre-fix code.
3. Apply the fix. Confirm the test passes.
4. The test stays in the suite permanently. Every incident becomes a regression check.