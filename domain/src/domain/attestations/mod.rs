/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `visit_attestations`, `attestation_attempts`,
/// `reviewer_assignment_log`, and `visit_signatures` (co-sign lifecycle).
pub mod repository;
/// Business logic for attestation initiation, signing, approval, and rejection (BFIP Section 6).
pub mod service;
