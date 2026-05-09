//! Whisked orders — customer pickup-ordering for matcha bars.
//!
//! Lifecycle: customer POSTs `/api/whisked/orders` → `pending` → staff moves
//! through `preparing` → `ready` → customer / staff `validate_pickup` consumes
//! the `W-XXXX` pickup code → `collected`. Cancellation is a separate path.
//!
//! The pickup code is the cryptographic-light handover artifact: customer
//! sees it on the iOS dashboard, staff verifies it via the validation endpoint,
//! and the partial unique index on `(pickup_code) WHERE pickup_code_used_at IS
//! NULL` prevents two simultaneous active orders from minting the same code.

/// Database row types, request bodies, and response shapes.
pub mod types;
/// SQL access for `whisked_orders` and `whisked_order_items`.
pub mod repository;
/// Service surface: place_order, get_order, validate_pickup, status transitions.
pub mod service;
