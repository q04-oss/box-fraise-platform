-- Migration 029: Drop remaining stale tables with no active domain.
--
-- table_bookings: table-reservation system (deleted with staff_web / admin).
-- referral_codes: referral program (never completed; no Rust handlers remain).
-- notifications:  push notification log (replaced by users.push_token + APNs
--                 direct calls; no Rust code reads this table).

DROP TABLE IF EXISTS table_bookings   CASCADE;
DROP TABLE IF EXISTS referral_codes   CASCADE;
DROP TABLE IF EXISTS notifications    CASCADE;
