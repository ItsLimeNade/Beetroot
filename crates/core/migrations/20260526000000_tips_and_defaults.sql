CREATE TABLE IF NOT EXISTS seen_tips (
    discord_id INTEGER NOT NULL,
    tip_id TEXT NOT NULL,
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (discord_id, tip_id)
);

CREATE INDEX IF NOT EXISTS idx_seen_tips_discord_id ON seen_tips(discord_id);

PRAGMA foreign_keys=off;

CREATE TABLE users_new (
    discord_id INTEGER PRIMARY KEY,
    nightscout_url TEXT,
    nightscout_token TEXT,
    allowed_people TEXT DEFAULT '[]',
    blocked_people TEXT DEFAULT '[]',
    is_private INTEGER DEFAULT 1,
    microbolus_threshold REAL DEFAULT 0.5,
    display_microbolus INTEGER DEFAULT 1,
    force_ephemeral INTEGER DEFAULT 0,
    mbg_expiry_time INTEGER DEFAULT 30,
    last_seen_version TEXT,
    bg_image_mode INTEGER DEFAULT 0
);

INSERT INTO users_new (
    discord_id,
    nightscout_url,
    nightscout_token,
    allowed_people,
    blocked_people,
    is_private,
    microbolus_threshold,
    display_microbolus,
    force_ephemeral,
    mbg_expiry_time,
    last_seen_version,
    bg_image_mode
)
SELECT
    discord_id,
    nightscout_url,
    nightscout_token,
    allowed_people,
    blocked_people,
    is_private,
    microbolus_threshold,
    display_microbolus,
    force_ephemeral,
    CASE WHEN mbg_expiry_time = 900 THEN 30 ELSE mbg_expiry_time END,
    last_seen_version,
    bg_image_mode
FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;

PRAGMA foreign_keys=on;
