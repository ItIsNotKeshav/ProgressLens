// ──────────────────────────────────────────────────────────────
// diff.rs — Snapshot diff engine
// ──────────────────────────────────────────────────────────────
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

use crate::models::{DiffResult, FieldDiff, StudentDiff, StudentDiffStatus};

/// Core row shape coming from the DB for each (student, field, value)
/// in a given snapshot.
struct ValueRow {
    student_id: i64,
    student_name: String,
    roll_number: String,
    sheet_key: String,
    field_label: String,
    value: String,
}

/// Compute per-student, per-field diffs between two snapshots.
pub async fn compute_diff(
    pool: &SqlitePool,
    snapshot_a: i64,
    snapshot_b: i64,
) -> Result<DiffResult, String> {
    let db_err = |e: sqlx::Error| e.to_string();

    // Fetch all values for snapshot A
    let rows_a = fetch_snapshot_values(pool, snapshot_a).await.map_err(db_err)?;
    // Fetch all values for snapshot B
    let rows_b = fetch_snapshot_values(pool, snapshot_b).await.map_err(db_err)?;

    // Build lookup: student_id → { field_key → (value, label) }
    let map_a = build_student_map(&rows_a);
    let map_b = build_student_map(&rows_b);

    // Build student info lookup
    let info_a = build_student_info(&rows_a);
    let info_b = build_student_info(&rows_b);

    // Collect all student IDs
    let all_students: HashSet<i64> = map_a.keys().chain(map_b.keys()).cloned().collect();

    // Collect all field keys across both snapshots for consistent ordering
    let all_fields: Vec<String> = {
        let mut keys: HashSet<String> = HashSet::new();
        for student_fields in map_a.values().chain(map_b.values()) {
            for k in student_fields.keys() {
                keys.insert(k.clone());
            }
        }
        let mut sorted: Vec<String> = keys.into_iter().collect();
        sorted.sort();
        sorted
    };

    let mut diffs: Vec<StudentDiff> = Vec::new();
    let mut total_changed: usize = 0;
    let mut total_added: usize = 0;
    let mut total_removed: usize = 0;

    for &student_id in &all_students {
        let in_a = map_a.contains_key(&student_id);
        let in_b = map_b.contains_key(&student_id);

        let (name, roll_number) = if in_b {
            info_b.get(&student_id).cloned().unwrap_or_default()
        } else {
            info_a.get(&student_id).cloned().unwrap_or_default()
        };

        let fields_a = map_a.get(&student_id);
        let fields_b = map_b.get(&student_id);

        if !in_a && in_b {
            // Student added in B
            total_added += 1;
            let field_diffs: Vec<FieldDiff> = all_fields
                .iter()
                .filter_map(|fk| {
                    let (val, label) = fields_b
                        .and_then(|f| f.get(fk))
                        .cloned()
                        .unwrap_or_default();
                    if val.is_empty() {
                        return None;
                    }
                    Some(FieldDiff {
                        field_key: fk.clone(),
                        label,
                        value_a: None,
                        value_b: Some(val),
                        changed: true,
                    })
                })
                .collect();

            diffs.push(StudentDiff {
                student_id,
                name,
                roll_number,
                status: StudentDiffStatus::Added,
                field_diffs,
            });
            continue;
        }

        if in_a && !in_b {
            // Student removed in B
            total_removed += 1;
            let field_diffs: Vec<FieldDiff> = all_fields
                .iter()
                .filter_map(|fk| {
                    let (val, label) = fields_a
                        .and_then(|f| f.get(fk))
                        .cloned()
                        .unwrap_or_default();
                    if val.is_empty() {
                        return None;
                    }
                    Some(FieldDiff {
                        field_key: fk.clone(),
                        label,
                        value_a: Some(val),
                        value_b: None,
                        changed: true,
                    })
                })
                .collect();

            diffs.push(StudentDiff {
                student_id,
                name,
                roll_number,
                status: StudentDiffStatus::Removed,
                field_diffs,
            });
            continue;
        }

        // Both present — compare field by field
        let mut field_diffs: Vec<FieldDiff> = Vec::new();
        let mut has_change = false;

        for fk in &all_fields {
            let (val_a, label_a) = fields_a
                .and_then(|f| f.get(fk))
                .cloned()
                .unwrap_or_default();
            let (val_b, label_b) = fields_b
                .and_then(|f| f.get(fk))
                .cloned()
                .unwrap_or_default();

            let label = if !label_b.is_empty() { label_b } else { label_a };
            let changed = val_a != val_b;
            if changed {
                has_change = true;
            }

            field_diffs.push(FieldDiff {
                field_key: fk.clone(),
                label,
                value_a: if val_a.is_empty() { None } else { Some(val_a) },
                value_b: if val_b.is_empty() { None } else { Some(val_b) },
                changed,
            });
        }

        if has_change {
            total_changed += 1;
        }

        let status = if has_change {
            StudentDiffStatus::Changed
        } else {
            StudentDiffStatus::Unchanged
        };

        diffs.push(StudentDiff {
            student_id,
            name,
            roll_number,
            status,
            field_diffs,
        });
    }

    // Sort: changed/added/removed first, unchanged last, then by roll number
    diffs.sort_by(|a, b| {
        let priority = |s: &StudentDiffStatus| -> u8 {
            match s {
                StudentDiffStatus::Added => 0,
                StudentDiffStatus::Removed => 1,
                StudentDiffStatus::Changed => 2,
                StudentDiffStatus::Unchanged => 3,
            }
        };
        priority(&a.status)
            .cmp(&priority(&b.status))
            .then_with(|| a.roll_number.cmp(&b.roll_number))
    });

    Ok(DiffResult {
        snapshot_a,
        snapshot_b,
        total_changed,
        total_added,
        total_removed,
        diffs,
    })
}

// ─── Internal helpers ──────────────────────────────────────────────────────

async fn fetch_snapshot_values(
    pool: &SqlitePool,
    snapshot_id: i64,
) -> Result<Vec<ValueRow>, sqlx::Error> {
    let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.name, s.roll_number, f.sheet_key, f.label, sv.value \
         FROM student_values sv \
         JOIN students s ON s.id = sv.student_id \
         JOIN fields f   ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? \
         ORDER BY s.roll_number, f.sheet_key",
    )
    .bind(snapshot_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(
            |(student_id, student_name, roll_number, sheet_key, field_label, value)| ValueRow {
                student_id,
                student_name,
                roll_number,
                sheet_key,
                field_label,
                value,
            },
        )
        .collect())
}

/// student_id → { sheet_key → (value, label) }
fn build_student_map(rows: &[ValueRow]) -> HashMap<i64, HashMap<String, (String, String)>> {
    let mut map: HashMap<i64, HashMap<String, (String, String)>> = HashMap::new();
    for r in rows {
        map.entry(r.student_id)
            .or_default()
            .insert(r.sheet_key.clone(), (r.value.clone(), r.field_label.clone()));
    }
    map
}

/// student_id → (name, roll_number)
fn build_student_info(rows: &[ValueRow]) -> HashMap<i64, (String, String)> {
    let mut map: HashMap<i64, (String, String)> = HashMap::new();
    for r in rows {
        map.entry(r.student_id)
            .or_insert_with(|| (r.student_name.clone(), r.roll_number.clone()));
    }
    map
}
