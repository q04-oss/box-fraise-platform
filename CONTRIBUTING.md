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

---

## Layer rules

The codebase is divided into three layers per domain. Each layer may only
call the layer immediately below it:

```
routes.rs  →  service.rs  →  repository.rs
```

**What each layer owns:**

| Layer | File | Responsibilities |
|---|---|---|
| Route | `routes.rs` | HTTP extraction, input validation, response shaping |
| Service | `service.rs` | Business logic, orchestration across repositories |
| Repository | `repository.rs` | Raw database queries, no business logic |
| Types | `types.rs` | Structs, enums, newtype IDs shared by the domain |

**Rules:**

- `routes.rs` must never import `repository` or touch `sqlx` directly.
- `service.rs` must never import `axum`, `Json`, `Router`, or any HTTP type.
- `repository.rs` must never import `axum` or contain business logic.
- `types.rs` has no import restrictions — it is a leaf module.

**Why this rule exists:**

- Routes stay thin and testable without a running server.
- Business rules live in one place; the HTTP layer is swappable.
- Repository queries are auditable without understanding HTTP or business
  logic.

**If you add a new domain:**

1. Create `service.rs` — even if it is a thin pass-through today.
2. Routes call service functions; service functions call repository
   functions.
3. Never skip a layer, even for "simple" one-liner operations.

---

## Command / Query separation

Every public service function is either a **command** or a **query**.
They are never mixed.

| Kind | Definition | Naming | Side effects |
|---|---|---|---|
| Command | Changes state (DB write, email, Redis mutation) | `verb_noun` — `send_message`, `register_user`, `update_display_name` | Allowed |
| Query | Reads state only | `get_noun` or `list_noun` — `get_active_user`, `list_notifications` | None |

**Rules:**

- A function that **returns data AND mutates state** must be split into a
  separate query and a separate command.
- The command may call the query internally if it needs to read before
  writing. The query must never call a command.
- Commands may return a minimal confirmation (a newly created entity's
  primary fields, an auth token that is the direct output of the command)
  but must not perform a separate read just to enrich the response.
- Read-then-write within a single conceptual operation (e.g. consume a
  token then update a row) is a command — that is not a violation.

**Naming quick reference:**

```
authenticate_apple   register_user        login_user
request_password_reset  reset_password   verify_email
request_magic_link   verify_magic_link   update_push_token
update_display_name  resend_verification_email
issue_challenge      register_keys        upload_otpks
archive_conversation send_message
mark_notification_read  mark_all_notifications_read

get_active_user      get_otpk_count       get_key_bundle
get_key_bundle_by_code  get_public_profile  get_social_access
get_system_prompt
list_conversations   list_notifications   search_users
get_thread
```

**Exception documented in code:**

`get_key_bundle` atomically claims one OTPK while returning the bundle.
These cannot be separated: X3DH protocol requires exactly one fresh
pre-key per session. The comment in `keys/service.rs` explains this.

---

## Error boundary

HTTP status codes are assigned exactly once, at the HTTP boundary, by
`server/src/error.rs`. Domain and integrations crates know nothing about
HTTP.

**`DomainError`** — `domain/src/error.rs`

- Plain Rust enum, no axum imports, no status codes.
- Variants describe domain states: `Unauthorized`, `Forbidden`,
  `InvalidInput`, `NotFound`, `Conflict`, `Unprocessable`,
  `RateLimitExceeded`, `PaymentRequired`, `ExternalServiceError`,
  `Internal`, `Db`.
- Helper constructors: `DomainError::invalid_input(msg)`,
  `DomainError::conflict(msg)`, `DomainError::unprocessable(msg)`.

**`AppError`** — `server/src/error.rs`

- HTTP-aware: implements `IntoResponse`, maps variants to status codes.
- Implements `From<DomainError>` so `?` propagation converts domain
  errors automatically at the route boundary.
- This is the ONLY type in the codebase that knows about HTTP status codes.

**Rules:**

- Never `use axum` or `use http` in `domain/` or `integrations/`.
- Never construct `AppError` inside `domain/` or `integrations/`.
- If you need a new error class, add a variant to `DomainError` and add
  the matching arm to `From<DomainError> for AppError` in
  `server/src/error.rs`.
- The `From` impl is the single mapping table — all status-code decisions
  live there and nowhere else.
