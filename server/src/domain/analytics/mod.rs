//! Internal analytics surface (Hardening §5).
//!
//! Read-only aggregate queries that power the admin dashboard, Grafana, and
//! Metabase. Every route is gated to `platform_admin`; every query is a
//! pure SELECT. No PII leaves these endpoints — responses are counts,
//! rates, and durations.

pub mod queries;
pub mod routes;
