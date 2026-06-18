-- Custom user themes (bonbon 0.4 theming).
-- `active_theme` on users selects the palette applied to glucose visuals.
-- NULL means the default dark theme. Otherwise it is either a builtin name
-- (e.g. 'beetroot_dark', 'licorice_dark') or 'custom:<name>' referencing a row
-- in the themes table below.
ALTER TABLE users ADD COLUMN active_theme TEXT;

CREATE TABLE IF NOT EXISTS themes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_id  INTEGER NOT NULL,
    name        TEXT NOT NULL,
    -- JSON object of all 14 bonbon Theme color fields as hex strings,
    -- example {"background":"#101015ff", "glucose_high":"#e3b10b" etc..}
    data        TEXT NOT NULL,
    created_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    UNIQUE(discord_id, name),
    FOREIGN KEY (discord_id) REFERENCES users(discord_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_themes_discord_id ON themes(discord_id);
