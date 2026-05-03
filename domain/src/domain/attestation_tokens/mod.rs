/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `attestation_tokens` and `third_party_verification_attempts`.
pub mod repository;
/// Business logic for token issuance, verification, and revocation (BFIP Section 11).
pub mod service;
