PRAGMA foreign_keys = OFF;

CREATE TABLE new_fields (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    sheet_id             INTEGER REFERENCES sheets(id) ON DELETE CASCADE,
    sheet_key            TEXT    NOT NULL,
    label                TEXT    NOT NULL,
    data_type            TEXT,
    is_visible           BOOLEAN NOT NULL DEFAULT 1,
    display_name         TEXT,
    max_value            REAL,
    include_in_dashboard BOOLEAN NOT NULL DEFAULT 0,
    UNIQUE (sheet_id, sheet_key)
);

INSERT INTO new_fields (id, sheet_id, sheet_key, label, data_type, is_visible)
SELECT id, sheet_id, sheet_key, label, NULL, is_visible FROM fields;
-- We set data_type to NULL by default to force re-classification of existing fields.

DROP TABLE fields;
ALTER TABLE new_fields RENAME TO fields;

PRAGMA foreign_keys = ON;
