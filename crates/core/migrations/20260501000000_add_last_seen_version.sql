-- Add last_seen_version to users.
--
-- The dashboard uses this column to decide whether to show the changelog
-- modal. NULL on existing rows means "never seen any version", so they'll
-- see the full changelog the next time they log in.
ALTER TABLE users ADD COLUMN last_seen_version TEXT;
