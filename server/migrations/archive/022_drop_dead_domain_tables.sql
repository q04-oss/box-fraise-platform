-- Migration 022: Drop tables for five deleted feature domains.
--
-- Domains removed: ventures, art, portal, standing_orders, contracts routes.
--
-- employment_contracts is intentionally NOT dropped — two live JOINs remain:
--   businesses::placed_user_push_token  (find on-duty worker's push token)
--   payments::complete_tip              (credit tip to the placed worker)
-- These cannot be replaced by devices.business_id (device ≠ shift assignment).
-- The contracts route domain (accept/decline API) is deleted; the table stays.
--
-- Drop order respects FK dependencies; CASCADE handles anything missed.

-- Art
DROP TABLE IF EXISTS art_bids                          CASCADE;
DROP TABLE IF EXISTS art_auctions                      CASCADE;
DROP TABLE IF EXISTS artworks                          CASCADE;
DROP TABLE IF EXISTS art_pitches                       CASCADE;

-- Ventures
DROP TABLE IF EXISTS venture_posts                     CASCADE;
DROP TABLE IF EXISTS venture_members                   CASCADE;
DROP TABLE IF EXISTS ventures                          CASCADE;

-- Portal
DROP TABLE IF EXISTS portal_access                     CASCADE;
DROP TABLE IF EXISTS portal_content                    CASCADE;
DROP TABLE IF EXISTS identity_verification_sessions    CASCADE;

-- Standing orders
DROP TABLE IF EXISTS standing_orders                   CASCADE;
