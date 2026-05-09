//! Billing — Stripe subscription lifecycle for `business_subscriptions`
//! (Hardening §10 / Grade A item 3).
//!
//! The webhook handler in [`service::handle_stripe_webhook`] is the single
//! write path: Stripe POSTs subscription create/update/delete events, this
//! domain verifies the HMAC, parses the event, and upserts a
//! [`types::BusinessSubscriptionRow`]. The admin read endpoint that lists
//! subscriptions lives in `platform_configuration::routes` for now and
//! pulls rows through this domain's repository.
pub mod repository;
pub mod service;
pub mod types;
