# Contributing to box-fraise-platform

## Environment variable access

**Rule:** `std::env::var`, `std::env::var_os`, and `std::env::vars` may
only be called inside `domain/src/config.rs`.

Every other file must obtain configuration values through the `Config`
struct. This is enforced by `cargo clippy` via `.clippy.toml` and
`[workspace.lints.clippy]` in `Cargo.toml`.

**If you need a new env variable:**

1. Add a field to `Config` in `domain/src/config.rs`.
2. Read it inside `Config::load()` using the existing `require()` /
   `optional()` helpers.
3. Mark secrets as `SecretString` — never `String`.
4. Validate at load time so the server fails fast on startup rather than
   failing on first use.
5. Reference the field via `state.cfg.your_field` in handlers.

**Why this rule exists:**

- A single load point means all configuration is validated once, at
  startup, with a clear error message.
- `SecretString` prevents secrets appearing in logs or `Debug` output.
- Code review for env var additions is trivial: look only at `config.rs`.
- The linter catches accidental bypasses automatically.

**The only exemption:**

`domain/src/config.rs` carries `#![allow(clippy::disallowed_methods)]`
at the top. Do not copy this attribute to any other file.
