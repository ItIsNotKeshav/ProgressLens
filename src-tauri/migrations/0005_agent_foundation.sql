-- Agent conversations, safe app-owned actions, and immutable audit history.

CREATE TABLE IF NOT EXISTS agent_conversations (
    id          TEXT PRIMARY KEY,
    sheet_id    INTEGER NOT NULL REFERENCES sheets(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT 'New conversation',
    summary     TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_messages (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id  TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    role             TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'tool')),
    content          TEXT NOT NULL,
    metadata_json    TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_agent_messages_conversation
    ON agent_messages(conversation_id, id);

CREATE TABLE IF NOT EXISTS mentor_notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    sheet_id    INTEGER NOT NULL REFERENCES sheets(id) ON DELETE CASCADE,
    student_id  INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
    note        TEXT NOT NULL CHECK (length(trim(note)) > 0),
    source      TEXT NOT NULL DEFAULT 'user',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS interventions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    sheet_id    INTEGER NOT NULL REFERENCES sheets(id) ON DELETE CASCADE,
    student_id  INTEGER NOT NULL REFERENCES students(id) ON DELETE CASCADE,
    status      TEXT NOT NULL CHECK (status IN ('required', 'active', 'resolved')),
    reason      TEXT NOT NULL,
    source      TEXT NOT NULL DEFAULT 'user',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS agent_operations (
    id                    TEXT PRIMARY KEY,
    conversation_id       TEXT REFERENCES agent_conversations(id) ON DELETE SET NULL,
    sheet_id              INTEGER NOT NULL REFERENCES sheets(id) ON DELETE CASCADE,
    expected_snapshot_id  INTEGER REFERENCES snapshots(id) ON DELETE SET NULL,
    kind                  TEXT NOT NULL,
    status                TEXT NOT NULL CHECK (status IN ('pending', 'confirmed', 'executed', 'cancelled', 'expired', 'failed')),
    payload_json          TEXT NOT NULL,
    preview_json          TEXT NOT NULL,
    expires_at            TEXT NOT NULL,
    created_at            TEXT NOT NULL DEFAULT (datetime('now')),
    executed_at           TEXT
);

CREATE TABLE IF NOT EXISTS audit_events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    operation_id    TEXT REFERENCES agent_operations(id) ON DELETE SET NULL,
    event_type      TEXT NOT NULL,
    actor           TEXT NOT NULL,
    details_json    TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

