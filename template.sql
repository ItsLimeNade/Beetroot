CREATE TABLE IF NOT EXISTS users (
    discord_id BIGINT,
    -- only used if is_private is set to true
    allowed_people TEXT DEFAULT '[]',
    -- allows other people to also view your blood glucose (set to true by default)
    is_private INTEGER NOT NULL DEFAULT 1,
    nightscout_url TEXT,
    -- used for treatments
    nightscout_token TEXT,
    -- fields
    PRIMARY KEY (discord_id)
);

CREATE TABLE IF NOT EXISTS stickers (
    id INT AUTOINCREMENT,
    -- images/stickers/$file_name
    file_name TEXT NOT NULL,
    discord_id BIGINT NOT NULL,
    -- fields
    PRIMARY KEY (id),
    FOREIGN KEY (discord_id) REFERENCES users(discord_id)
);