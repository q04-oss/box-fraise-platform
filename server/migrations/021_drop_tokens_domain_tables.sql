-- Migration 021: Drop tables for the deleted tokens domain.
--
-- evening_tokens, portrait_tokens, portrait_purchase_intents, and trade_offers
-- were all part of the tokens domain (evening token social proofs, portrait NFT
-- marketplace, content token trading). The domain is fully removed from the
-- codebase — no routes, handlers, or repository code remain.
--
-- trade_offers and bookings have no migration history (added to production
-- without a migration); DROP IF EXISTS is safe.

DROP TABLE IF EXISTS portrait_purchase_intents CASCADE;
DROP TABLE IF EXISTS portrait_tokens           CASCADE;
DROP TABLE IF EXISTS trade_offers              CASCADE;
DROP TABLE IF EXISTS evening_tokens            CASCADE;
DROP TABLE IF EXISTS bookings                  CASCADE;
