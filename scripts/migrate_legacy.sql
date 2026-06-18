-- Legacy data migration for Beetroot.
--
-- Copies users and stickers from the pre-rewrite database (db.sqlite) into the
-- current schema (beetroot.db). Safe to run more than once: existing rows are
-- never overwritten and stickers are de-duplicated by (discord_id, url).
--
-- Run from the repository root, against the NEW database:
--
--   sqlite3 data/beetroot.db ".read scripts/migrate_legacy.sql"
--
-- BEFORE RUNNING:
--   * Stop the bot so nothing writes mid-migration.
--   * Back up data/beetroot.db (e.g. copy it aside).
--   * Keep the SAME ENCRYPTION_SALT (or ENCRYPTION_KEY with that value) the old
--     bot used, otherwise the copied Nightscout tokens will not decrypt.

ATTACH DATABASE 'data/db.sqlite' AS legacy;

PRAGMA foreign_keys = ON;

BEGIN;

-- Users: bring over legacy rows that don't already exist in the new database.
-- "OR IGNORE" means anyone who already set up on the new bot is left untouched.
-- New-only columns (blocked_people, force_ephemeral, mbg_expiry_time,
-- command_count, treatment_mode, graph_sticker_count, ...) fall back to their
-- schema defaults. nightscout_token is copied verbatim: the new crypto derives
-- the same AES-256-GCM key from the same salt, so it still decrypts.
INSERT OR IGNORE INTO users (
    discord_id,
    nightscout_url,
    nightscout_token,
    allowed_people,
    is_private,
    microbolus_threshold,
    display_microbolus,
    last_seen_version
)
SELECT
    discord_id,
    nightscout_url,
    nightscout_token,
    allowed_people,
    is_private,
    microbolus_threshold,
    display_microbolus,
    last_seen_version
FROM legacy.users;

-- Stickers: file_name -> sticker_url, remap the renamed categories, and turn the
-- old empty-string display names into NULL. Only import stickers for users that
-- exist in the new table (FK safety), and skip any URL the user already has so
-- re-running this script never creates duplicates. The legacy position columns
-- (x_position / y_position / rotation) are intentionally dropped: the current
-- schema has no per-sticker placement.
INSERT INTO stickers (discord_id, sticker_url, display_name, category)
SELECT
    s.discord_id,
    s.file_name,
    NULLIF(s.display_name, ''),
    CASE s.category
        WHEN 'inrange'  THEN 'in_range'
        WHEN 'any'      THEN 'background'
        WHEN 'low'      THEN 'low'
        WHEN 'high'     THEN 'high'
        -- pre-rewrite never wrote these, but map them defensively anyway
        WHEN 'rising'   THEN 'fast_rise'
        WHEN 'dropping' THEN 'fast_drop'
        WHEN 'other'    THEN 'background'
        ELSE 'background'
    END AS category
FROM legacy.stickers AS s
WHERE EXISTS (SELECT 1 FROM users u WHERE u.discord_id = s.discord_id)
  AND NOT EXISTS (
        SELECT 1 FROM stickers t
        WHERE t.discord_id = s.discord_id
          AND t.sticker_url = s.file_name
      );

COMMIT;

DETACH DATABASE legacy;
