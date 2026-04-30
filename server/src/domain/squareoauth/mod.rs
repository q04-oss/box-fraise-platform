/// Square OAuth domain — merchant account connection and token lifecycle.
///
/// Public surface used by other domains:
///   `service::load_decrypted(state, business_id)` — returns live tokens,
///   refreshing transparently when within 24h of expiry.
pub mod repository;
pub mod routes;
pub mod service;
