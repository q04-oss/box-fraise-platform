-- Migration 016: Add verified and owner_id to businesses.
--
-- These columns were added directly to the production database without a
-- migration, creating schema drift. This migration formalises them so fresh
-- test databases match production.
--
-- verified:  whether the business has passed platform onboarding review.
-- owner_id:  the user who owns/administers the business account.

ALTER TABLE businesses
    ADD COLUMN IF NOT EXISTS verified  BOOLEAN  NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS owner_id  INTEGER  REFERENCES users(id) ON DELETE SET NULL;
