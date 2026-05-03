/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — single source of truth for all SQL touching `background_checks`.
pub mod repository;
/// Business logic for background check initiation, webhook processing, and status queries.
pub mod service;
