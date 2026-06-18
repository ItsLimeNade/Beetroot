CREATE TABLE IF NOT EXISTS dashboard_sessions (
    id TEXT PRIMARY KEY,              -- 256-bit random hex token
    discord_id INTEGER NOT NULL,
    discord_username TEXT NOT NULL,
    discord_avatar TEXT,
    created_at INTEGER NOT NULL,      -- Unix epoch seconds
    last_active_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_discord_id ON dashboard_sessions(discord_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON dashboard_sessions(expires_at);
