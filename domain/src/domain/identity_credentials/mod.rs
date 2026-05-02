/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — single source of truth for all SQL touching
/// `identity_credentials` and `cooling_period_events`.
pub mod repository;
/// Business logic for identity verification and cooling-period tracking.
pub mod service;
