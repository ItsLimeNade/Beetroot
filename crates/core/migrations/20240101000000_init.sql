-- 1. Users Table
CREATE TABLE IF NOT EXISTS users (
    discord_id INTEGER PRIMARY KEY,
    
    -- Nightscout
    nightscout_url TEXT,
    nightscout_token TEXT,
    
    -- Social
    allowed_people TEXT DEFAULT '[]',
    blocked_people TEXT DEFAULT '[]',
    is_private INTEGER DEFAULT 1, -- Boolean
    
    -- Preferences
    microbolus_threshold REAL DEFAULT 0.5,
    display_microbolus INTEGER DEFAULT 1, -- Boolean
    force_ephemeral INTEGER DEFAULT 0,    -- Boolean
    mbg_expiry_time INTEGER DEFAULT 900   -- Seconds (default 15 mins)
);

CREATE TABLE IF NOT EXISTS stickers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    discord_id INTEGER NOT NULL,
    sticker_url TEXT NOT NULL,
    display_name TEXT,
    category TEXT NOT NULL, -- 'in_range', 'low', 'high', 'other'
    FOREIGN KEY (discord_id) REFERENCES users(discord_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS command_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command_name TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    execution_time_ms INTEGER NOT NULL,
    created_at INTEGER NOT NULL -- Unix timestamp
);

CREATE TABLE IF NOT EXISTS bot_stats (
    key TEXT PRIMARY KEY,
    value TEXT
);