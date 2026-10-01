-- Add graph_denoise to users.
--
-- How strongly glucose readings are smoothed before being drawn on a graph:
-- 0 = off (raw readings), 1 = light, 2 = medium, 3 = strong.
ALTER TABLE users ADD COLUMN graph_denoise INTEGER NOT NULL DEFAULT 0;
