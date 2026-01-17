CREATE TABLE IF NOT EXISTS replay_cursors (
    client_id VARCHAR(255) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    cursor BIGINT NOT NULL,
    PRIMARY KEY (client_id, topic)
);

CREATE TABLE IF NOT EXISTS replay_events (
    client_id VARCHAR(255) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    cursor BIGINT NOT NULL,
    payload LONGBLOB NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (client_id, topic, cursor)
);

CREATE INDEX idx_replay_events_lookup
ON replay_events (client_id, topic, cursor);
