-- no-transaction
-- Migration 013: Drop 119 dormant tables with zero Rust code references
--
-- Method: word-boundary grep (\btable_name\b) across all .rs files.
-- Tables with zero matches have no SQL queries, no type mappings, and no
-- domain handlers — they are fully dormant.
--
-- FK notes — four live tables hold nullable FK columns pointing into this set.
-- CASCADE removes those FK *constraints* only; the live tables, their columns,
-- and their existing row data are untouched:
--   orders.batch_id          → batches
--   orders.menu_item_id      → business_menu_items
--   evening_tokens.offer_id  → reservation_offers
--   table_bookings.event_id  → table_events
--
-- Drop order: children before parents within each domain cluster, though
-- CASCADE makes the order formally irrelevant.

-- ── Advertising ───────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS ad_impressions          CASCADE;
DROP TABLE IF EXISTS ad_campaigns            CASCADE;

-- ── Akene ─────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS akene_purchases         CASCADE;
DROP TABLE IF EXISTS akene_invitations       CASCADE;
DROP TABLE IF EXISTS akene_events            CASCADE;

-- ── AR / editorial ────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS ar_notes                CASCADE;
DROP TABLE IF EXISTS editorial_pieces        CASCADE;

-- ── Art ───────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS art_management_fees     CASCADE;
DROP TABLE IF EXISTS art_acquisitions        CASCADE;
-- art_auctions, art_bids, art_pitches, artworks are live (≥1 rs ref) — kept

-- ── Batches / bundles ─────────────────────────────────────────────────────────
DROP TABLE IF EXISTS batch_preferences       CASCADE;
DROP TABLE IF EXISTS batches                 CASCADE;  -- drops orders.batch_id FK
DROP TABLE IF EXISTS bundle_orders           CASCADE;
DROP TABLE IF EXISTS bundle_varieties        CASCADE;

-- ── Beacons ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS beacons                 CASCADE;

-- ── Business extras ───────────────────────────────────────────────────────────
DROP TABLE IF EXISTS business_accounts       CASCADE;
DROP TABLE IF EXISTS business_menu_items     CASCADE;  -- drops orders.menu_item_id FK
DROP TABLE IF EXISTS business_promotions     CASCADE;
DROP TABLE IF EXISTS business_proposals      CASCADE;
DROP TABLE IF EXISTS business_visits         CASCADE;

-- ── Campaigns extras ──────────────────────────────────────────────────────────
DROP TABLE IF EXISTS campaign_commissions    CASCADE;
-- campaigns, campaign_signups are live — kept

-- ── Co-scans ──────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS co_scans                CASCADE;

-- ── Collectifs ────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS collectif_challenges    CASCADE;
DROP TABLE IF EXISTS collectif_commitments   CASCADE;
DROP TABLE IF EXISTS collectifs              CASCADE;

-- ── Community ─────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS community_events        CASCADE;
DROP TABLE IF EXISTS community_fund_contributions CASCADE;
DROP TABLE IF EXISTS community_fund          CASCADE;
DROP TABLE IF EXISTS community_popup_interest CASCADE;

-- ── Connections (social graph) ────────────────────────────────────────────────
DROP TABLE IF EXISTS pending_connections     CASCADE;
DROP TABLE IF EXISTS connections             CASCADE;

-- ── Contracts ─────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS contract_requests       CASCADE;

-- ── Conversations ─────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS conversation_archives   CASCADE;

-- ── Corporate ─────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS corporate_members       CASCADE;
DROP TABLE IF EXISTS corporate_accounts      CASCADE;

-- ── Credits ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS credit_transactions     CASCADE;

-- ── Dates ─────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS date_invitations        CASCADE;
DROP TABLE IF EXISTS date_matches            CASCADE;
DROP TABLE IF EXISTS date_offers             CASCADE;

-- ── DJ ────────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS dj_offers               CASCADE;

-- ── Drops ─────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS drop_claims             CASCADE;
DROP TABLE IF EXISTS drop_waitlist           CASCADE;
DROP TABLE IF EXISTS drops                   CASCADE;

-- ── Explicit portals ──────────────────────────────────────────────────────────
DROP TABLE IF EXISTS explicit_portals        CASCADE;

-- ── Farm ──────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS farm_visit_bookings     CASCADE;
DROP TABLE IF EXISTS farm_visits             CASCADE;

-- ── Fraise legacy system ──────────────────────────────────────────────────────
DROP TABLE IF EXISTS fraise_business_sessions CASCADE;
DROP TABLE IF EXISTS fraise_claims           CASCADE;
DROP TABLE IF EXISTS fraise_credit_purchases CASCADE;
DROP TABLE IF EXISTS fraise_interest         CASCADE;
DROP TABLE IF EXISTS fraise_invitations      CASCADE;
DROP TABLE IF EXISTS fraise_member_resets    CASCADE;
DROP TABLE IF EXISTS fraise_member_sessions  CASCADE;
DROP TABLE IF EXISTS fraise_messages         CASCADE;
DROP TABLE IF EXISTS fraise_businesses       CASCADE;
DROP TABLE IF EXISTS fraise_events           CASCADE;
DROP TABLE IF EXISTS fraise_members          CASCADE;

-- ── Gifts ─────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS gift_registry           CASCADE;
-- gifts is live — kept

-- ── Greenhouse ────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS greenhouse_funding      CASCADE;
DROP TABLE IF EXISTS greenhouses             CASCADE;

-- ── Harvest ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS harvest_logs            CASCADE;

-- ── Health ────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS health_profiles         CASCADE;

-- ── Identity audit (legacy) ───────────────────────────────────────────────────
DROP TABLE IF EXISTS id_attestation_log      CASCADE;

-- ── Itineraries ───────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS itinerary_proposals     CASCADE;
DROP TABLE IF EXISTS itinerary_destinations  CASCADE;
DROP TABLE IF EXISTS itineraries             CASCADE;

-- ── Jobs ──────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS job_ledger_entries      CASCADE;
DROP TABLE IF EXISTS job_interviews          CASCADE;
DROP TABLE IF EXISTS job_applications        CASCADE;
DROP TABLE IF EXISTS job_postings            CASCADE;

-- ── Kommune ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS kommune_assignments     CASCADE;
DROP TABLE IF EXISTS kommune_flavour_suggestions CASCADE;
DROP TABLE IF EXISTS kommune_press_applications  CASCADE;
DROP TABLE IF EXISTS kommune_ratings         CASCADE;
DROP TABLE IF EXISTS kommune_reservations    CASCADE;

-- ── Location extras ───────────────────────────────────────────────────────────
DROP TABLE IF EXISTS location_funding        CASCADE;
DROP TABLE IF EXISTS location_staff          CASCADE;

-- ── Market ────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS market_order_items      CASCADE;
DROP TABLE IF EXISTS market_orders_v2        CASCADE;
DROP TABLE IF EXISTS market_orders           CASCADE;
DROP TABLE IF EXISTS market_listings         CASCADE;
DROP TABLE IF EXISTS market_vendors          CASCADE;
DROP TABLE IF EXISTS market_products         CASCADE;
DROP TABLE IF EXISTS market_stalls           CASCADE;
DROP TABLE IF EXISTS market_dates            CASCADE;

-- ── Meeting tokens ────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS meeting_tokens          CASCADE;

-- ── Memory ────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS memory_requests         CASCADE;

-- ── NFC extras ────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS nfc_pairing_tokens      CASCADE;

-- ── Nodes ─────────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS node_applications       CASCADE;

-- ── Order extras ──────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS order_splits            CASCADE;

-- ── Personal toilets ──────────────────────────────────────────────────────────
DROP TABLE IF EXISTS toilet_visits           CASCADE;
DROP TABLE IF EXISTS personal_toilets        CASCADE;

-- ── Personalized menus ────────────────────────────────────────────────────────
DROP TABLE IF EXISTS personalized_menus      CASCADE;

-- ── Platform messages (legacy) ────────────────────────────────────────────────
DROP TABLE IF EXISTS platform_messages       CASCADE;

-- ── Popup extras ──────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS popup_food_orders       CASCADE;
DROP TABLE IF EXISTS popup_merch_orders      CASCADE;
DROP TABLE IF EXISTS popup_merch_items       CASCADE;
DROP TABLE IF EXISTS popup_requests          CASCADE;

-- ── Portal extras ─────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS portal_consents         CASCADE;

-- ── Portrait extras ───────────────────────────────────────────────────────────
DROP TABLE IF EXISTS portrait_license_requests CASCADE;
DROP TABLE IF EXISTS portrait_licenses       CASCADE;
DROP TABLE IF EXISTS portrait_token_listings CASCADE;
DROP TABLE IF EXISTS portraits               CASCADE;

-- ── Preorders ─────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS preorders               CASCADE;

-- ── Product bundles ───────────────────────────────────────────────────────────
DROP TABLE IF EXISTS product_bundles         CASCADE;

-- ── Promotions ────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS promotion_deliveries    CASCADE;

-- ── Provenance ────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS provenance_tokens       CASCADE;

-- ── Referrals ─────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS referrals               CASCADE;

-- ── Reservations ──────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS reservation_bookings    CASCADE;
DROP TABLE IF EXISTS reservation_offers      CASCADE;  -- drops evening_tokens.offer_id FK

-- ── Seasons ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS season_patronages       CASCADE;

-- ── Staff sessions (legacy) ───────────────────────────────────────────────────
DROP TABLE IF EXISTS staff_sessions          CASCADE;

-- ── Standing orders extras ────────────────────────────────────────────────────
DROP TABLE IF EXISTS standing_order_tiers    CASCADE;
DROP TABLE IF EXISTS standing_order_transfers CASCADE;
DROP TABLE IF EXISTS standing_order_waitlist CASCADE;

-- ── Table bookings extras ─────────────────────────────────────────────────────
DROP TABLE IF EXISTS table_booking_tokens    CASCADE;
DROP TABLE IF EXISTS table_events            CASCADE;  -- drops table_bookings.event_id FK
DROP TABLE IF EXISTS table_instructors       CASCADE;
DROP TABLE IF EXISTS table_memberships       CASCADE;
DROP TABLE IF EXISTS table_venue_sessions    CASCADE;
DROP TABLE IF EXISTS table_venues            CASCADE;

-- ── Tasting ───────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS tasting_feed_reactions  CASCADE;
DROP TABLE IF EXISTS tasting_journal         CASCADE;

-- ── Typing indicators ─────────────────────────────────────────────────────────
DROP TABLE IF EXISTS typing_indicators       CASCADE;

-- ── User extras ───────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS user_business_visits    CASCADE;
DROP TABLE IF EXISTS user_challenge_progress CASCADE;
DROP TABLE IF EXISTS user_earnings           CASCADE;
DROP TABLE IF EXISTS user_map_entries        CASCADE;
DROP TABLE IF EXISTS user_maps               CASCADE;
DROP TABLE IF EXISTS user_saves              CASCADE;

-- ── Variety extras ────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS variety_drops           CASCADE;
DROP TABLE IF EXISTS variety_profiles        CASCADE;
DROP TABLE IF EXISTS variety_reviews         CASCADE;
DROP TABLE IF EXISTS variety_seasons         CASCADE;

-- ── Verification payments ─────────────────────────────────────────────────────
DROP TABLE IF EXISTS verification_payments   CASCADE;

-- ── Walk-in tokens ────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS walk_in_tokens          CASCADE;

-- ── Webhooks ──────────────────────────────────────────────────────────────────
DROP TABLE IF EXISTS webhook_subscriptions   CASCADE;
