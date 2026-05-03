/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `support_bookings` and `gift_box_history`.
pub mod repository;
/// Business logic for support booking lifecycle and gift box fulfilment (BFIP Section 10).
pub mod service;
