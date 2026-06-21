CREATE TABLE IF NOT EXISTS agent_settings (
    id                  INTEGER PRIMARY KEY CHECK (id = 1),
    enabled             BOOLEAN NOT NULL DEFAULT 1,
    ollama_endpoint     TEXT NOT NULL DEFAULT 'http://localhost:11434',
    model_name          TEXT NOT NULL DEFAULT 'qwen3:8b',
    timeout_seconds     INTEGER NOT NULL DEFAULT 90 CHECK (timeout_seconds BETWEEN 10 AND 300),
    max_tool_iterations INTEGER NOT NULL DEFAULT 4 CHECK (max_tool_iterations BETWEEN 1 AND 8),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT OR IGNORE INTO agent_settings (id) VALUES (1);

CREATE TABLE IF NOT EXISTS agent_tool_calls (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id  TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    tool_name        TEXT NOT NULL,
    arguments_json   TEXT NOT NULL,
    result_json      TEXT,
    status           TEXT NOT NULL CHECK (status IN ('started', 'completed', 'failed')),
    error_message    TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_agent_tool_calls_conversation
    ON agent_tool_calls(conversation_id, id);

