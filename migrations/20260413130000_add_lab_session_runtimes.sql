CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS lab_session_runtimes (
    runtime_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES lab_sessions(session_id) ON DELETE CASCADE,
    container_id TEXT UNIQUE,
    runtime_kind TEXT NOT NULL CHECK (runtime_kind IN ('terminal', 'web')),
    status TEXT NOT NULL CHECK (status IN ('created', 'starting', 'running', 'stopped', 'expired', 'error')),
    namespace TEXT NOT NULL,
    webshell_url TEXT,
    app_url TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMP NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMP NOT NULL,
    stopped_at TIMESTAMP NULL,
    restart_index INTEGER NOT NULL
);

ALTER TABLE lab_sessions
    ADD COLUMN IF NOT EXISTS current_runtime_id UUID NULL,
    ADD COLUMN IF NOT EXISTS completed_at TIMESTAMP NULL,
    ADD COLUMN IF NOT EXISTS last_activity_at TIMESTAMP NULL;

UPDATE lab_sessions
SET last_activity_at = COALESCE(last_activity_at, created_at)
WHERE last_activity_at IS NULL;

ALTER TABLE lab_sessions
    ALTER COLUMN last_activity_at SET NOT NULL;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'lab_sessions_current_runtime_id_fkey'
    ) THEN
        ALTER TABLE lab_sessions
            ADD CONSTRAINT lab_sessions_current_runtime_id_fkey
            FOREIGN KEY (current_runtime_id)
            REFERENCES lab_session_runtimes(runtime_id)
            ON DELETE SET NULL;
    END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS lab_sessions_one_active_per_user_lab
    ON lab_sessions(user_id, lab_id)
    WHERE status IN ('created', 'in_progress');

CREATE UNIQUE INDEX IF NOT EXISTS lab_session_runtimes_one_active_per_session
    ON lab_session_runtimes(session_id)
    WHERE status IN ('created', 'starting', 'running');

CREATE UNIQUE INDEX IF NOT EXISTS lab_session_runtimes_session_restart_index
    ON lab_session_runtimes(session_id, restart_index);
