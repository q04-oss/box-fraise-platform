/// Venue drinks domain — menu, in-app ordering, and Stripe Connect onboarding.
///
/// Public surface used by the payments webhook handler:
///   `service::complete_venue_order(state, pi_id)` — called on payment_intent.succeeded
///   when metadata["type"] == "venue_order". Fire-and-forget: errors are logged,
///   not propagated back to Stripe (always returns 200 to prevent retries from
///   blocking unrelated events in the same batch).
pub mod repository;
pub mod routes;
pub mod service;
pub mod types;
