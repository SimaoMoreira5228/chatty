CREATE TABLE IF NOT EXISTS command_audit (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    client_id VARCHAR(255) NOT NULL,
    topic VARCHAR(255) NOT NULL,
    command_kind VARCHAR(64) NOT NULL,
    target_user_id VARCHAR(255),
    target_message_id VARCHAR(255),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_command_audit_topic
ON command_audit (topic);

CREATE INDEX idx_command_audit_created_at
ON command_audit (created_at);

CREATE TABLE IF NOT EXISTS connection_sessions (
    session_id VARCHAR(255) PRIMARY KEY,
    client_id VARCHAR(255) NOT NULL,
    remote_addr VARCHAR(255),
    started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at TIMESTAMP NULL
);

CREATE INDEX idx_connection_sessions_client
ON connection_sessions (client_id);
