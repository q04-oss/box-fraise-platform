#![deny(missing_docs)]
//! Domain crate for the box-fraise platform.
//!
//! Contains all business logic, database access, event bus infrastructure,
//! and shared configuration. No HTTP or web-framework types live here.

/// Append-only audit event writer backed by the `audit_events` table.
pub mod audit;
/// JWT signing/verification, token revocation, and Apple Sign In verification.
pub mod auth;
/// Server configuration loaded from environment variables at startup.
pub mod config;
/// AES-256-GCM application-layer encryption for sensitive database values.
pub mod crypto;
/// Postgres connection pool factory.
pub mod db;
/// Business domain modules (auth, messages, keys, users, dorotka).
pub mod domain;
/// Domain-level error types (`DomainError`, `AppResult`).
pub mod error;
/// In-process broadcast event bus for decoupled cross-domain communication.
pub mod event_bus;
/// Domain event enum — all event variants emitted by the platform.
pub mod events;
/// Newtype ID wrappers and shared value types.
pub mod types;
