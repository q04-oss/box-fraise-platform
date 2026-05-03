/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `soultokens` and `soultoken_renewals`.
pub mod repository;
/// Business logic for soultoken issuance, revocation, surrender, and renewal (BFIP Section 7).
pub mod service;
