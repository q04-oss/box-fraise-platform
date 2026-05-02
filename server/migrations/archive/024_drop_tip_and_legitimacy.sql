-- Migration 024: Drop tables for deleted tip and legitimacy features.
--
-- Tip payments: complete_tip removed from webhook dispatcher; tip handler
--   removed from businesses routes; placed_user_push_token removed (no callers).
--   employment_contracts has zero remaining JOINs — dropped here.
--   expire_contracts cron job also removed.
--
-- Legitimacy: checkin handler removed from popups routes; popup_checkins and
--   legitimacy_events have zero remaining writes.
--
-- Drop order respects FK dependencies; CASCADE handles anything missed.

DROP TABLE IF EXISTS earnings_ledger               CASCADE;
DROP TABLE IF EXISTS tip_payments                  CASCADE;
DROP TABLE IF EXISTS employment_contracts          CASCADE;
DROP TABLE IF EXISTS legitimacy_events             CASCADE;
DROP TABLE IF EXISTS popup_checkins                CASCADE;
