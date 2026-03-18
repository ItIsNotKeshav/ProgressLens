// ──────────────────────────────────────────────────────────────
// snapshot.rs — Snapshot creation: upsert students/fields,
//               insert student_values for a given SheetData.
// ──────────────────────────────────────────────────────────────
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::sheets::SheetData;

/// Persist a SheetData into the database, creating a new snapshot.
///
/// Steps:
///   1. Upsert fields table with detected column headers
///   2. Create a new snapshots row
///   3. Upsert students by roll_number (never duplicate)
///   4. Insert student_values for every cell in this snapshot
///
/// Returns the new snapshot_id.
pub async fn save_snapshot(
    pool: &SqlitePool,
    sheet_id: i64,
    data: &SheetData,
    roll_number_key: &str,
    name_key: &str,
) -> Result<i64, String> {
    let db_err = |e: sqlx::Error| e.to_string();

    // ── 1. Upsert fields ───────────────────────────────────────
    // Map sheet_key → field_id for later use
    let mut field_map: HashMap<String, i64> = HashMap::new();

    for header in &data.headers {
        // Skip the roll_number and name columns — they are student identity,
        // not tracked field values
        if header.field_key == roll_number_key || header.field_key == name_key {
            continue;
        }

        // On subsequent syncs, DO NOTHING if it exists, to preserve user-edited labels and types.
        sqlx::query(
            "INSERT INTO fields (sheet_id, sheet_key, label, data_type, is_visible) \
             VALUES (?, ?, ?, ?, 1) \
             ON CONFLICT(sheet_id, sheet_key) DO NOTHING",
        )
        .bind(sheet_id)
        .bind(&header.field_key)
        .bind(&header.label)
        .bind(&header.data_type)
        .execute(pool)
        .await
        .map_err(db_err)?;

        let field_id: i64 =
            sqlx::query_scalar("SELECT id FROM fields WHERE sheet_id = ? AND sheet_key = ?")
                .bind(sheet_id)
                .bind(&header.field_key)
                .fetch_one(pool)
                .await
                .map_err(db_err)?;

        field_map.insert(header.field_key.clone(), field_id);
    }

    // ── 2. Create snapshot ─────────────────────────────────────
    let snapshot_id: i64 = sqlx::query_scalar(
        "INSERT INTO snapshots (sheet_id, source_label) VALUES (?, ?) RETURNING id",
    )
    .bind(sheet_id)
    .bind(&data.source_label)
    .fetch_one(pool)
    .await
    .map_err(db_err)?;

    // ── 3‑4. Upsert students + insert values ──────────────────
    let mut students_upserted: usize = 0;

    for row in &data.rows {
        let roll_number = row.get(roll_number_key).map(|s| s.as_str()).unwrap_or("");
        let name = row.get(name_key).map(|s| s.as_str()).unwrap_or("");

        if roll_number.is_empty() {
            continue; // skip rows with no roll number
        }

        // Upsert student
        sqlx::query(
            "INSERT INTO students (name, roll_number) \
             VALUES (?, ?) \
             ON CONFLICT(roll_number) DO UPDATE SET name = excluded.name",
        )
        .bind(name)
        .bind(roll_number)
        .execute(pool)
        .await
        .map_err(db_err)?;

        students_upserted += 1;

        let student_id: i64 =
            sqlx::query_scalar("SELECT id FROM students WHERE roll_number = ?")
                .bind(roll_number)
                .fetch_one(pool)
                .await
                .map_err(db_err)?;

        // Insert student_values for every tracked field
        for (field_key, field_id) in &field_map {
            let value = row.get(field_key).map(|s| s.as_str()).unwrap_or("");

            sqlx::query(
                "INSERT INTO student_values (student_id, field_id, snapshot_id, value) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT(student_id, field_id, snapshot_id) \
                 DO UPDATE SET value = excluded.value",
            )
            .bind(student_id)
            .bind(field_id)
            .bind(snapshot_id)
            .bind(value)
            .execute(pool)
            .await
            .map_err(db_err)?;
        }
    }

    log::info!(
        "Snapshot {} saved: {} students, {} fields",
        snapshot_id,
        students_upserted,
        field_map.len()
    );

    Ok(snapshot_id)
}

// ─── Auto-detect identity columns ─────────────────────────────────────────

/// Try to identify which columns are the "roll number" and "name"
/// by looking at the header labels.
pub fn detect_identity_columns(data: &SheetData) -> (String, String) {
    let mut roll_key = String::new();
    let mut name_key = String::new();

    for h in &data.headers {
        let lower = h.label.to_lowercase();
        if roll_key.is_empty()
            && (lower.contains("roll")
                || lower.contains("id")
                || lower.contains("enrollment")
                || lower.contains("enrolment")
                || lower.contains("reg"))
        {
            roll_key = h.field_key.clone();
        }
        if name_key.is_empty()
            && (lower.contains("name")
                || lower.contains("student"))
            && !lower.contains("roll")
        {
            name_key = h.field_key.clone();
        }
    }

    // Fallback: first column is roll, second is name
    if roll_key.is_empty() {
        if let Some(h) = data.headers.first() {
            roll_key = h.field_key.clone();
        }
    }
    if name_key.is_empty() {
        if let Some(h) = data.headers.get(1) {
            name_key = h.field_key.clone();
        }
    }

    (roll_key, name_key)
}
