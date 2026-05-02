-- Migration 023: Drop tables for deleted feature domains.
--
-- social graph:        user_follows
-- membership waitlist: membership_waitlist
-- community fund:      fund_contributions, membership_funds
-- nominations:         popup_nominations
-- campaigns:           campaign_signups, campaigns
-- venue drinks + sq:   venue_orders, venue_drinks, square_oauth_tokens
-- device pairing:      device_pairing_tokens

DROP TABLE IF EXISTS user_follows            CASCADE;
DROP TABLE IF EXISTS membership_waitlist     CASCADE;
DROP TABLE IF EXISTS fund_contributions      CASCADE;
DROP TABLE IF EXISTS membership_funds        CASCADE;
DROP TABLE IF EXISTS popup_nominations       CASCADE;
DROP TABLE IF EXISTS campaign_signups        CASCADE;
DROP TABLE IF EXISTS campaigns               CASCADE;
DROP TABLE IF EXISTS venue_orders            CASCADE;
DROP TABLE IF EXISTS venue_drinks            CASCADE;
DROP TABLE IF EXISTS square_oauth_tokens     CASCADE;
DROP TABLE IF EXISTS device_pairing_tokens   CASCADE;
