-- Migration 0001: initial schema
-- ProgressLens point-in-time student tracker

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- Core student registry
CREATE TABLE IF NOT EXISTS students (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    roll_number TEXT    NOT NULL UNIQUE,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- Dynamic field definitions (columns discovered from the sheet)
CREATE TABLE IF NOT EXISTS fields (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    label      TEXT    NOT NULL,          -- human-readable name, e.g. "Marks Obtained"
    field_key  TEXT    NOT NULL UNIQUE,   -- normalized key, e.g. "marks_obtained"
    data_type  TEXT    NOT NULL DEFAULT 'text' -- 'text' | 'number' | 'date'
);

-- Each sync produces one snapshot
CREATE TABLE IF NOT EXISTS snapshots (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    synced_at    TEXT    NOT NULL DEFAULT (datetime('now')),
    source_label TEXT    NOT NULL         -- e.g. "Sheet: Sem 3 Marks"
);

-- Actual data: one row per (student × field × snapshot)
CREATE TABLE IF NOT EXISTS student_values (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    student_id  INTEGER NOT NULL REFERENCES students(id)  ON DELETE CASCADE,
    field_id    INTEGER NOT NULL REFERENCES fields(id)    ON DELETE CASCADE,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    value       TEXT    NOT NULL DEFAULT '',
    UNIQUE (student_id, field_id, snapshot_id)
);

-- Indexes for point-in-time queries
CREATE INDEX IF NOT EXISTS idx_sv_snapshot ON student_values(snapshot_id);
CREATE INDEX IF NOT EXISTS idx_sv_student  ON student_values(student_id);
CREATE INDEX IF NOT EXISTS idx_sv_field    ON student_values(field_id);
