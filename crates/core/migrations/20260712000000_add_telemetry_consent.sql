-- Whether the user consented to usage telemetry (the command_logs table).
-- NULL = never asked, 1 = opted in, 0 = opted out. Telemetry is only recorded
-- when this is 1, so the NULL default means no collection until a user agrees.
ALTER TABLE users ADD COLUMN telemetry_accepted INTEGER;
