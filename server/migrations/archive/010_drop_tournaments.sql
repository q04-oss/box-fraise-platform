-- Migration 010: Remove tournament tables
--
-- The tournament domain has been removed from the codebase. Drop all three
-- tables in dependency order (plays → entries → tournaments). CASCADE is
-- included as a safety net in case any FK or view was added later.

DROP TABLE IF EXISTS tournament_plays    CASCADE;
DROP TABLE IF EXISTS tournament_entries  CASCADE;
DROP TABLE IF EXISTS tournaments         CASCADE;
