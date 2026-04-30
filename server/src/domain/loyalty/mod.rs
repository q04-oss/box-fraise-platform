/// Loyalty domain — per-business stamp programme.
///
/// Public surface used by other domains (venue_drinks webhook handler):
///   `service::record_steep_from_webhook(state, user_id, business_id, idempotency_key)`
pub mod repository;
pub mod routes;
pub mod service;
pub mod types;
