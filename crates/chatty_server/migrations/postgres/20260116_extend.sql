CREATE TABLE IF NOT EXISTS command_audit (
    id BIGSERIAL PRIMARY KEY,
    client_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    command_kind TEXT NOT NULL,
    target_user_id TEXT,
    target_message_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_command_audit_topic
ON command_audit (topic);

CREATE INDEX IF NOT EXISTS idx_command_audit_created_at
ON command_audit (created_at);

CREATE TABLE IF NOT EXISTS connection_sessions (
    session_id TEXT PRIMARY KEY,
    client_id TEXT NOT NULL,
    remote_addr TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_connection_sessions_client
ON connection_sessions (client_id);
