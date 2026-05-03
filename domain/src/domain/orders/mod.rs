/// Database row types, column lists, and HTTP request/response shapes.
pub mod types;
/// Database access — all SQL touching `orders` and `visit_boxes`.
pub mod repository;
/// Business logic for order creation, NFC collection, and box activation (BFIP Section 9).
pub mod service;
