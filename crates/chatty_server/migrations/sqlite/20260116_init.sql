CREATE TABLE IF NOT EXISTS replay_cursors (
    client_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    cursor INTEGER NOT NULL,
    PRIMARY KEY (client_id, topic)
);

CREATE TABLE IF NOT EXISTS replay_events (
    client_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    cursor INTEGER NOT NULL,
    payload BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (client_id, topic, cursor)
);

CREATE INDEX IF NOT EXISTS idx_replay_events_lookup
ON replay_events (client_id, topic, cursor);
