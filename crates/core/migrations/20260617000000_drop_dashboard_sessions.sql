-- The bundled web dashboard was removed, so its session table is dead. Dropped
-- here (rather than deleting the create migration) to keep migration history
-- append-only, so databases that already applied it are cleaned up safely.
DROP TABLE IF EXISTS dashboard_sessions;
