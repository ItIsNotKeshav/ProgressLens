PRAGMA foreign_keys = OFF;

CREATE TABLE new_fields (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    sheet_id   INTEGER REFERENCES sheets(id) ON DELETE CASCADE,
    sheet_key  TEXT    NOT NULL,
    label      TEXT    NOT NULL,
    data_type  TEXT    NOT NULL DEFAULT 'text',
    is_visible BOOLEAN NOT NULL DEFAULT 1,
    UNIQUE (sheet_id, sheet_key)
);

INSERT INTO new_fields (id, sheet_key, label, data_type)
SELECT id, field_key, label, data_type FROM fields;

DROP TABLE fields;
ALTER TABLE new_fields RENAME TO fields;

PRAGMA foreign_keys = ON;
