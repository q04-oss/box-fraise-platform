-- Migration 028: Drop dead columns from the users table.
--
-- These columns belonged to deleted features (portal, worker/staff system,
-- identity attestation, Stripe Connect, ad platform, platform credits).
-- No Rust code reads or writes any of them. The fraise_chat_email unique
-- constraint is automatically dropped with its column.

ALTER TABLE users
    DROP COLUMN IF EXISTS campaign_interest,
    DROP COLUMN IF EXISTS photographed,
    DROP COLUMN IF EXISTS fraise_chat_email,
    DROP COLUMN IF EXISTS identity_session_id,
    DROP COLUMN IF EXISTS id_attested_by,
    DROP COLUMN IF EXISTS id_attested_at,
    DROP COLUMN IF EXISTS id_attestation_expires_at,
    DROP COLUMN IF EXISTS verification_renewal_due_at,
    DROP COLUMN IF EXISTS stripe_connect_account_id,
    DROP COLUMN IF EXISTS stripe_connect_onboarded,
    DROP COLUMN IF EXISTS ad_balance_cents,
    DROP COLUMN IF EXISTS platform_credit_cents,
    DROP COLUMN IF EXISTS portal_opted_in,
    DROP COLUMN IF EXISTS worker_status;
