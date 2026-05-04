/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `verification_events` and `audit_request_log`.
pub mod repository;
/// Business logic for audit trail assembly and user data access rights (BFIP Section 17).
pub mod service;
