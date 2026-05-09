//! Whisked menu — public catalogue of drinks served by Whisked bars.
//!
//! Single read endpoint; no mutations from the customer iOS client. Menu
//! items are managed via SQL admin until a CMS endpoint lands.
/// Database row types and column lists for `whisked_menu_items`.
pub mod types;
/// SQL access for `whisked_menu_items`.
pub mod repository;
/// Public service surface — `list_menu` returns every available item.
pub mod service;
