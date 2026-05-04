/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `platform_configuration` and `platform_configuration_history`.
pub mod repository;
/// Business logic for configuration reads, updates, and seeding (BFIP Section 15).
pub mod service;
