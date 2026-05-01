//! Auth integration tests — Redis-dependent.
//!
//! verify_email and resend_verification were removed along with the
//! email/password auth system. Tests for the remaining auth surface
//! (magic link, Apple Sign In) are in handler.rs.

mod common;
