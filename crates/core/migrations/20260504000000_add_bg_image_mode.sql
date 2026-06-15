-- Add bg_image_mode to users.
--
-- When true, /bg sends a BgCard PNG image instead of a Discord embed.
-- NULL on existing rows defaults to false (classic embed behavior).
ALTER TABLE users ADD COLUMN bg_image_mode BOOL DEFAULT 0;
