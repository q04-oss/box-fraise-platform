-- Migration 027: Drop tables for the 12 deleted domains.
--
-- Domains removed: businesses, catalog, orders, payments, popups, gifts,
--   loyalty, memberships, nfc, devices, search, staff_web.
--
-- Drop order: children before parents (CASCADE also handles any missed FKs).

-- ── Devices ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS devices              CASCADE;
DROP TABLE IF EXISTS device_attestations  CASCADE;
DROP TABLE IF EXISTS attest_challenges    CASCADE;

-- ── Orders ────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS orders               CASCADE;

-- ── Popups ────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS popup_rsvps          CASCADE;

-- ── Gifts ─────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS gifts                CASCADE;

-- ── Loyalty ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS loyalty_events           CASCADE;
DROP TABLE IF EXISTS business_loyalty_config  CASCADE;
DROP TABLE IF EXISTS nfc_stickers             CASCADE;

-- ── Memberships ───────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS memberships          CASCADE;

-- ── NFC connections ───────────────────────────────────────────────────────────
DROP TABLE IF EXISTS nfc_connections      CASCADE;

-- ── Catalog ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS time_slots           CASCADE;
DROP TABLE IF EXISTS varieties            CASCADE;

-- ── Businesses / Locations (drop after child tables) ─────────────────────────
DROP TABLE IF EXISTS locations            CASCADE;
DROP TABLE IF EXISTS businesses           CASCADE;

-- ── Orphaned ENUM types ───────────────────────────────────────────────────────
-- gift_tone was used only by gifts (dropped above).
-- The remaining types were already orphaned by earlier migrations
-- (022–024) that dropped standing_orders, campaigns, etc.
DROP TYPE IF EXISTS public.gift_tone                  CASCADE;
DROP TYPE IF EXISTS public.campaign_status            CASCADE;
DROP TYPE IF EXISTS public.campaign_signup_status     CASCADE;
DROP TYPE IF EXISTS public.social_tier                CASCADE;
DROP TYPE IF EXISTS public.standing_order_frequency   CASCADE;
DROP TYPE IF EXISTS public.standing_order_status      CASCADE;
DROP TYPE IF EXISTS public.location_staff_status      CASCADE;
DROP TYPE IF EXISTS public.batch_preference_status    CASCADE;
DROP TYPE IF EXISTS public.editorial_status           CASCADE;
