-- Add bg_sticker to users.
--
-- When true, the /bg embed shows one of the user's stickers (picked to match
-- their current glucose state) in place of their profile picture.
-- NULL on existing rows defaults to false (profile picture behavior).
ALTER TABLE users ADD COLUMN bg_sticker BOOL DEFAULT 0;
