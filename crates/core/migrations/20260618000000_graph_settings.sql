-- Per-user /graph rendering preferences.
-- treatment_mode: how treatments are drawn ('contextual' or 'timeline').
-- graph_sticker_count: how many stickers to scatter on the graph (0 to 30).
ALTER TABLE users ADD COLUMN treatment_mode TEXT NOT NULL DEFAULT 'contextual';
ALTER TABLE users ADD COLUMN graph_sticker_count INTEGER NOT NULL DEFAULT 8;
