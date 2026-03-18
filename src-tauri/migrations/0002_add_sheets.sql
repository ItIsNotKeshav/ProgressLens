-- Migration 0002: add sheets
CREATE TABLE IF NOT EXISTS sheets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    url         TEXT    NOT NULL UNIQUE,
    label       TEXT    NOT NULL,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

ALTER TABLE snapshots ADD COLUMN sheet_id INTEGER REFERENCES sheets(id) ON DELETE CASCADE;
