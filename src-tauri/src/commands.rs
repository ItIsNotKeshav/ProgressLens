// ──────────────────────────────────────────────────────────────
// commands.rs — Tauri invoke handlers (fully wired)
// ──────────────────────────────────────────────────────────────
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::State;

use crate::auth;
use crate::diff;
use crate::models::{
    AvgScore, DashboardStats, DiffResult, Field, FieldConfig, LevelCount, LevelStats, LevelTrackCount, RecentChange, ReportConfig, Snapshot,
    StudentLevelRow, StudentRow, SyncResult, TopPerformer,
};
use crate::sheets;
use crate::snapshot;
use crate::sync_worker::SyncState;

/// Wraps sqlx errors as strings for Tauri's serializable error path.
fn db_err(e: sqlx::Error) -> String {
    e.to_string()
}

// ─── authenticate_google ───────────────────────────────────────────────────

/// Trigger Google OAuth2 flow.  If a stored refresh token exists, silently
/// refreshes.  Otherwise opens the browser for consent.
/// Returns true if authenticated successfully.
#[tauri::command]
pub async fn authenticate_google(
    data_dir: State<'_, PathBuf>,
) -> Result<bool, String> {
    let _token = auth::get_access_token(&data_dir).await?;
    Ok(true)
}

// ─── sync_from_sheet ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn preview_sheet(
    sheet_url: String,
    data_dir: State<'_, PathBuf>,
) -> Result<Vec<FieldConfig>, String> {
    log::info!("preview_sheet: {}", sheet_url);
    let access_token = auth::get_access_token(&data_dir).await?;
    let sheet_data = sheets::fetch_sheet(&sheet_url, &access_token).await?;

    let (roll_key, name_key) = snapshot::detect_identity_columns(&sheet_data);

    let configs: Vec<FieldConfig> = sheet_data.headers.into_iter().filter_map(|h| {
        if h.field_key == roll_key || h.field_key == name_key {
            None
        } else {
            Some(FieldConfig {
                sheet_key: h.field_key,
                label: h.label,
                data_type: h.data_type,
                is_visible: true,
            })
        }
    }).collect();

    Ok(configs)
}

#[tauri::command]
pub async fn sync_from_sheet(
    sheet_url: String,
    configs: Vec<FieldConfig>,
    sheet_label: Option<String>,
    data_dir: State<'_, PathBuf>,
    db: State<'_, SqlitePool>,
) -> Result<SyncResult, String> {
    log::info!("sync_from_sheet called with url: {}", sheet_url);

    // 1. Get a valid access token
    let access_token = auth::get_access_token(&data_dir).await?;

    // 2. Fetch the sheet via Sheets API v4
    let sheet_data = sheets::fetch_sheet(&sheet_url, &access_token).await?;

    let fields_detected = sheet_data.headers.len();
    let source_label = sheet_data.source_label.clone();
    let display_label = sheet_label.unwrap_or_else(|| source_label.clone());
    let db_pool = db.inner();

    // 3. Find or create sheet (use custom label if provided)
    sqlx::query("INSERT INTO sheets (url, label) VALUES (?, ?) ON CONFLICT(url) DO UPDATE SET label = excluded.label")
        .bind(&sheet_url)
        .bind(&display_label)
        .execute(db_pool)
        .await
        .map_err(db_err)?;

    let sheet_id: i64 = sqlx::query_scalar("SELECT id FROM sheets WHERE url = ?")
        .bind(&sheet_url)
        .fetch_one(db_pool)
        .await
        .map_err(db_err)?;

    // 4. Auto-detect identity columns (still needed for snapshot payload mapping)
    let (roll_key, name_key) = snapshot::detect_identity_columns(&sheet_data);

    // 5. Upsert field configurations
    for config in configs {
        sqlx::query(
            "INSERT INTO fields (sheet_id, sheet_key, label, data_type, is_visible) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(sheet_id, sheet_key) DO UPDATE SET \
                label = excluded.label, \
                data_type = excluded.data_type, \
                is_visible = excluded.is_visible"
        )
        .bind(sheet_id)
        .bind(&config.sheet_key)
        .bind(&config.label)
        .bind(&config.data_type)
        .bind(config.is_visible)
        .execute(db_pool)
        .await
        .map_err(db_err)?;
    }

    // 6. Persist everything as a new snapshot
    let students_upserted = sheet_data.rows.len();
    let snapshot_result =
        snapshot::save_snapshot(db_pool, sheet_id, &sheet_data, &roll_key, &name_key).await?;
    let snapshot_id = snapshot_result.snapshot_id;

    // 7. Auto-classify fields
    auto_classify_fields(db_pool, sheet_id).await?;

    Ok(SyncResult {
        sheet_id,
        snapshot_id,
        students_upserted,
        fields_detected,
        source_label,
        synced_at: chrono::Utc::now().to_rfc3339(),
    })
}

// ─── sync_sheet ────────────────────────────────────────────────────────────

/// Re-syncs an existing sheet by its ID
#[tauri::command]
pub async fn sync_sheet(
    sheet_id: i64,
    data_dir: State<'_, PathBuf>,
    db: State<'_, SqlitePool>,
) -> Result<SyncResult, String> {
    log::info!("sync_sheet called for sheet_id: {}", sheet_id);
    let db_pool = db.inner();

    let sheet_url: String = sqlx::query_scalar("SELECT url FROM sheets WHERE id = ?")
        .bind(sheet_id)
        .fetch_optional(db_pool)
        .await
        .map_err(db_err)?
        .ok_or("Sheet not found in database.")?;

    let access_token = auth::get_access_token(&data_dir).await?;
    let sheet_data = sheets::fetch_sheet(&sheet_url, &access_token).await?;

    let fields_detected = sheet_data.headers.len();
    let source_label = sheet_data.source_label.clone();
    
    // update label just in case
    sqlx::query("UPDATE sheets SET label = ? WHERE id = ?")
        .bind(&source_label)
        .bind(sheet_id)
        .execute(db_pool)
        .await
        .map_err(db_err)?;

    let (roll_key, name_key) = snapshot::detect_identity_columns(&sheet_data);
    let students_upserted = sheet_data.rows.len();
    let snapshot_result =
        snapshot::save_snapshot(db_pool, sheet_id,  &sheet_data, &roll_key, &name_key).await?;
    let snapshot_id = snapshot_result.snapshot_id;

    auto_classify_fields(db_pool, sheet_id).await?;

    Ok(SyncResult {
        sheet_id,
        snapshot_id,
        students_upserted,
        fields_detected,
        source_label,
        synced_at: chrono::Utc::now().to_rfc3339(),
    })
}

// ─── get_sheets ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_sheets(db: State<'_, SqlitePool>) -> Result<Vec<crate::models::Sheet>, String> {
    log::info!("get_sheets");
    let rows: Vec<(i64, String, String, String)> = sqlx::query_as("SELECT id, url, label, created_at FROM sheets ORDER BY id ASC")
        .fetch_all(db.inner())
        .await
        .map_err(db_err)?;
    Ok(rows.into_iter().map(|(id, url, label, created_at)| crate::models::Sheet { id, url, label, created_at }).collect())
}

// ─── get_diff ──────────────────────────────────────────────────────────────

/// Compare two snapshots and return per-student field-level diffs.
#[tauri::command]
pub async fn get_diff(
    snapshot_a: i64,
    snapshot_b: i64,
    db: State<'_, SqlitePool>,
) -> Result<DiffResult, String> {
    log::info!("get_diff: {} vs {}", snapshot_a, snapshot_b);
    diff::compute_diff(db.inner(), snapshot_a, snapshot_b).await
}

// ─── get_all_students ──────────────────────────────────────────────────────

/// Return all students with their field values for the given snapshot.
/// If snapshot_id is None, uses the latest snapshot.
#[tauri::command]
pub async fn get_all_students(
    sheet_id: i64,
    snapshot_id: Option<i64>,
    db: State<'_, SqlitePool>,
) -> Result<Vec<StudentRow>, String> {
    log::info!("get_all_students: sheet={}, snapshot={:?}", sheet_id, snapshot_id);

    let snap_id: Option<i64> = match snapshot_id {
        Some(id) => Some(id),
        None => {
            sqlx::query_scalar("SELECT id FROM snapshots WHERE sheet_id = ? ORDER BY id DESC LIMIT 1")
                .bind(sheet_id)
                .fetch_optional(db.inner())
                .await
                .map_err(db_err)?
        }
    };

    let snap_id = match snap_id {
        Some(id) => id,
        None => return Ok(vec![]), // no snapshots yet
    };

    // Fetch all (student, sheet_key, value) triples for this snapshot
    let rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.name, s.roll_number, f.sheet_key, sv.value \
         FROM student_values sv \
         JOIN students s ON s.id = sv.student_id \
         JOIN fields f   ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? \
         ORDER BY s.roll_number, f.sheet_key",
    )
    .bind(snap_id)
    .fetch_all(db.inner())
    .await
    .map_err(db_err)?;

    // Pivot into StudentRow structs
    let mut student_map: HashMap<i64, StudentRow> = HashMap::new();

    for (id, name, roll_number, sheet_key, value) in rows {
        let entry = student_map.entry(id).or_insert_with(|| StudentRow {
            id,
            name,
            roll_number,
            values: HashMap::new(),
        });
        entry.values.insert(sheet_key, value);
    }

    // Also include students that exist but have no values in this snapshot
    let all_students: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, name, roll_number FROM students ORDER BY roll_number")
            .fetch_all(db.inner())
            .await
            .map_err(db_err)?;

    for (id, name, roll_number) in all_students {
        student_map.entry(id).or_insert_with(|| StudentRow {
            id,
            name,
            roll_number,
            values: HashMap::new(),
        });
    }

    let mut students: Vec<StudentRow> = student_map.into_values().collect();
    students.sort_by(|a, b| a.roll_number.cmp(&b.roll_number));

    Ok(students)
}

// ─── get_fields ──────────────────────────────────────────────────────────────

/// Fetch all registered fields to let users pick columns for the report.
#[tauri::command]
pub async fn get_fields(db: State<'_, SqlitePool>) -> Result<Vec<Field>, String> {
    log::info!("get_fields");
    let rows: Vec<(i64, Option<i64>, String, String, Option<String>, bool, Option<String>, Option<f64>, bool)> = sqlx::query_as(
        "SELECT id, sheet_id, label, sheet_key, data_type, is_visible, display_name, max_value, include_in_dashboard FROM fields ORDER BY id"
    )
        .fetch_all(db.inner())
        .await
        .map_err(db_err)?;
    
    Ok(rows.into_iter().map(|(id, sheet_id, label, sheet_key, data_type, is_visible, display_name, max_value, include_in_dashboard)| Field {
        id, sheet_id, label, sheet_key, data_type, is_visible, display_name, max_value, include_in_dashboard
    }).collect())
}

#[tauri::command]
pub async fn get_field_setup(sheet_id: i64, db: State<'_, SqlitePool>) -> Result<Vec<crate::models::FieldSetup>, String> {
    let rows: Vec<(i64, String, String, Option<String>, Option<String>, Option<f64>, bool, bool)> = sqlx::query_as(
        "SELECT id, sheet_key, label, display_name, data_type, max_value, is_visible, include_in_dashboard FROM fields WHERE sheet_id = ? ORDER BY id"
    ).bind(sheet_id).fetch_all(db.inner()).await.map_err(db_err)?;

    let mut result = Vec::new();
    for (id, sheet_key, label, display_name, data_type, max_value, is_visible, include_in_dashboard) in rows {
        let display_name = display_name.unwrap_or(label.clone());
        let sample_values: Vec<String> = sqlx::query_scalar("SELECT value FROM student_values WHERE field_id = ? AND value != '' LIMIT 3")
            .bind(id)
            .fetch_all(db.inner()).await.map_err(db_err)?;

        result.push(crate::models::FieldSetup {
            id, sheet_key, label, display_name, data_type, max_value, is_visible, include_in_dashboard, sample_values
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn save_field_setup(fields: Vec<crate::models::FieldSetup>, db: State<'_, SqlitePool>) -> Result<(), String> {
    let mut tx = db.inner().begin().await.map_err(db_err)?;
    for field in fields {
        sqlx::query("UPDATE fields SET data_type = ?, display_name = ?, max_value = ?, is_visible = ?, include_in_dashboard = ? WHERE id = ?")
            .bind(&field.data_type)
            .bind(&field.display_name)
            .bind(field.max_value)
            .bind(field.is_visible)
            .bind(field.include_in_dashboard)
            .bind(field.id)
            .execute(&mut *tx).await.map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;
    Ok(())
}

async fn auto_classify_fields(db: &SqlitePool, sheet_id: i64) -> Result<(), String> {
    let fields: Vec<(i64, String, String)> = sqlx::query_as("SELECT id, sheet_key, label FROM fields WHERE sheet_id = ? AND data_type IS NULL")
        .bind(sheet_id)
        .fetch_all(db).await.map_err(db_err)?;

    for (field_id, sheet_key, label) in fields {
        let samples: Vec<String> = sqlx::query_scalar("SELECT value FROM student_values WHERE field_id = ? AND value != '' LIMIT 20")
            .bind(field_id)
            .fetch_all(db).await.map_err(db_err)?;

        let mut data_type = "text";
        let mut max_value: Option<f64> = None;
        let mut include_dashboard = false;

        let key_lower = sheet_key.to_lowercase();
        
        let mut is_link = false;
        let mut is_identifier = false;
        let mut is_score = false;
        let mut is_level = false;
        let mut is_categorical = false;

        if key_lower.contains("name") || key_lower.contains("usn") || key_lower.contains("roll") || key_lower.contains("email") || key_lower.contains("phone") || key_lower.contains("section") {
            is_identifier = true;
        } else {
            let http_count = samples.iter().filter(|s| s.starts_with("http")).count();
            if !samples.is_empty() && http_count * 2 > samples.len() {
                is_link = true;
            } else {
                let level_keys = vec!["level", "ax", "sx", "cx", "px", "l4", "tyl"];
                if level_keys.iter().any(|&k| key_lower.contains(k)) {
                    let mut all_levels = true;
                    for s in &samples {
                        if let Ok(num) = s.parse::<i32>() {
                            if num < 0 || num > 6 { all_levels = false; }
                        } else {
                            all_levels = false;
                        }
                    }
                    if !samples.is_empty() && all_levels {
                        is_level = true;
                    }
                }

                if !is_level {
                    let score_keys = vec!["score", "marks", "cgpa", "gpa"];
                    let has_score_key = score_keys.iter().any(|&k| key_lower.contains(k))
                        || key_lower.ends_with("_5") || key_lower.ends_with("_10") || key_lower.ends_with("_100");
                    if has_score_key {
                        let mut numeric_count = 0;
                        for s in &samples {
                            if s.parse::<f64>().is_ok() { numeric_count += 1; }
                        }
                        if !samples.is_empty() && numeric_count * 2 > samples.len() {
                            is_score = true;
                        }
                    }
                }

                if !is_level && !is_score {
                    use std::collections::HashSet;
                    let unique: HashSet<&String> = samples.iter().collect();
                    if !samples.is_empty() && unique.len() < 10 {
                        let mut all_short = true;
                        for s in &unique {
                            if s.len() > 30 { all_short = false; }
                        }
                        if all_short {
                            is_categorical = true;
                        }
                    }
                }
            }
        }

        if is_identifier {
            data_type = "identifier";
            include_dashboard = false;
        } else if is_link {
            data_type = "link";
            include_dashboard = false;
        } else if is_level {
            data_type = "level";
            max_value = Some(4.0);
            include_dashboard = true;
        } else if is_score {
            data_type = "score";
            include_dashboard = true;
            if key_lower.ends_with("_5") || key_lower.contains("_5_") {
                max_value = Some(5.0);
            } else if key_lower.ends_with("_10") || key_lower.contains("_10_") {
                max_value = Some(10.0);
            } else if key_lower.ends_with("_100") || key_lower.contains("_100_") {
                max_value = Some(100.0);
            } else {
                max_value = Some(100.0);
            }
        } else if is_categorical {
            data_type = "categorical";
            include_dashboard = false;
        }

        let display_name_str = label.clone();

        sqlx::query("UPDATE fields SET data_type = ?, max_value = ?, include_in_dashboard = ?, display_name = ? WHERE id = ?")
            .bind(data_type)
            .bind(max_value)
            .bind(include_dashboard)
            .bind(&display_name_str)
            .bind(field_id)
            .execute(db).await.map_err(db_err)?;
    }
    Ok(())
}

// ─── get_snapshots ───────────────────────────────────────────────────────────

/// Fetch all snapshots ordered by id (chronological)
#[tauri::command]
pub async fn get_snapshots(
    sheet_id: i64,
    db: State<'_, SqlitePool>,
) -> Result<Vec<Snapshot>, String> {
    log::info!("get_snapshots tracking sheet {}", sheet_id);

    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, synced_at, source_label FROM snapshots WHERE sheet_id = ? ORDER BY id ASC",
    )
    .bind(sheet_id)
    .fetch_all(db.inner())
    .await
    .map_err(db_err)?;

    Ok(rows
        .into_iter()
        .map(|(id, synced_at, source_label)| Snapshot {
            id,
            synced_at,
            source_label,
        })
        .collect())
}

// ─── get_dashboard_stats ───────────────────────────────────────────────────

/// Aggregate counts and the latest snapshot for the dashboard view.
#[tauri::command]
pub async fn get_dashboard_stats(
    sheet_id: i64,
    snapshot_id: Option<i64>,
    db: State<'_, SqlitePool>,
) -> Result<DashboardStats, String> {
    log::info!("get_dashboard_stats: sheet={}, snapshot={:?}", sheet_id, snapshot_id);

    let db_pool = db.inner();

    let snap_id = match snapshot_id {
        Some(id) => id,
        None => {
            let latest: Option<i64> = sqlx::query_scalar("SELECT id FROM snapshots WHERE sheet_id = ? ORDER BY id DESC LIMIT 1")
                .bind(sheet_id)
                .fetch_optional(db_pool)
                .await
                .map_err(db_err)?;
            match latest {
                Some(id) => id,
                None => {
                    return Ok(DashboardStats {
                        total_students: 0,
                        active_this_week: 0,
                        avg_score_per_field: vec![],
                        level_distribution: vec![],
                        recent_changes: vec![],
                        top_performers: vec![],
                    });
                }
            }
        }
    };

    let total_students: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT student_id) FROM student_values WHERE snapshot_id = ?")
        .bind(snap_id)
        .fetch_one(db_pool)
        .await
        .unwrap_or(0);

    let active_this_week: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT sv.student_id) \
         FROM student_values sv \
         JOIN snapshots snap ON snap.id = sv.snapshot_id \
         WHERE snap.synced_at >= datetime('now', '-7 days')"
    )
    .fetch_one(db_pool)
    .await
    .unwrap_or(0);

    let avg_rows: Vec<(String, f64)> = sqlx::query_as(
        "SELECT COALESCE(f.display_name, f.label), COALESCE(AVG(CAST(sv.value AS REAL)), 0.0) \
         FROM student_values sv JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND f.data_type = 'score' AND f.include_in_dashboard = 1 AND sv.value != '' \
         GROUP BY f.id"
    )
    .bind(snap_id)
    .fetch_all(db_pool)
    .await
    .unwrap_or_default();
    
    let avg_score_per_field = avg_rows.into_iter().map(|(field_label, avg)| AvgScore { field_label, avg }).collect();

    let level_rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT COALESCE(f.display_name, f.label), sv.value, COUNT(sv.student_id) \
         FROM student_values sv JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND f.data_type = 'level' AND sv.value != '' \
         GROUP BY f.id, sv.value"
    )
    .bind(snap_id)
    .fetch_all(db_pool)
    .await
    .unwrap_or_default();

    let level_distribution = level_rows.into_iter().map(|(field_label, level, count)| LevelCount { field_label, level, count }).collect();

    let top_rows: Vec<(String, f64, String)> = sqlx::query_as(
        "SELECT s.name, CAST(sv.value AS REAL), COALESCE(f.display_name, f.label) \
         FROM student_values sv \
         JOIN students s ON s.id = sv.student_id \
         JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND f.data_type = 'score' AND sv.value != '' \
         ORDER BY CAST(sv.value AS REAL) DESC LIMIT 5"
    )
    .bind(snap_id)
    .fetch_all(db_pool)
    .await
    .unwrap_or_default();

    let top_performers = top_rows.into_iter().map(|(name, score, field_label)| TopPerformer { name, score, field_label }).collect();

    let recent_rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT s.name, f.label, \
            COALESCE(prev_sv.value, ''), sv.value, \
            snap.synced_at \
         FROM student_values sv \
         JOIN snapshots snap ON snap.id = sv.snapshot_id \
         JOIN students s ON s.id = sv.student_id \
         JOIN fields f ON f.id = sv.field_id \
         JOIN (SELECT id FROM snapshots WHERE id <= ? ORDER BY id DESC LIMIT 5) recent_snaps \
           ON recent_snaps.id = sv.snapshot_id \
         LEFT JOIN student_values prev_sv ON prev_sv.student_id = sv.student_id \
                                      AND prev_sv.field_id = sv.field_id \
                                      AND prev_sv.snapshot_id = ( \
                                          SELECT MAX(id) FROM snapshots WHERE id < sv.snapshot_id \
                                      ) \
         WHERE prev_sv.value IS NOT NULL AND prev_sv.value != sv.value \
         ORDER BY sv.snapshot_id DESC, s.name ASC, f.label ASC LIMIT 10"
    )
    .bind(snap_id)
    .fetch_all(db_pool)
    .await
    .unwrap_or_default();

    let recent_changes = recent_rows.into_iter().map(|(student_name, field_label, old_val, new_val, synced_at)| RecentChange {
        student_name, field_label, old_val, new_val, synced_at
    }).collect();

    Ok(DashboardStats {
        total_students,
        active_this_week,
        avg_score_per_field,
        level_distribution,
        recent_changes,
        top_performers,
    })
}

// ─── get_level_stats ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_level_stats(
    snapshot_id: i64,
    db: State<'_, SqlitePool>,
) -> Result<LevelStats, String> {
    log::info!("get_level_stats: snapshot_id={}", snapshot_id);
    let db_pool = db.inner();

    // 1. Get all fields of type 'level' (and optionally include_in_dashboard = 1 if desired, but we'll get all levels)
    // Actually per user, "for each 'level' typed field".
    let level_fields: Vec<(i64, String, Option<f64>)> = sqlx::query_as(
        "SELECT id, COALESCE(display_name, label), max_value FROM fields WHERE data_type = 'level' ORDER BY id"
    )
    .fetch_all(db_pool)
    .await
    .map_err(db_err)?;

    let mut per_track: HashMap<i64, LevelTrackCount> = HashMap::new();
    let mut field_max: HashMap<i64, i32> = HashMap::new();

    for (f_id, d_name, max_val) in level_fields {
        let max = max_val.unwrap_or(4.0) as i32;
        field_max.insert(f_id, max);
        let mut counts = HashMap::new();
        // Initialize counts 1..max ?
        for i in 1..=4 {
            counts.insert(i, 0);
        }
        per_track.insert(f_id, LevelTrackCount {
            field_id: f_id,
            display_name: d_name,
            counts,
        });
    }

    // 2. Get students mapped to their levels
    let rows: Vec<(i64, String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT s.id, s.name, s.roll_number, f.id, sv.value \
         FROM students s \
         CROSS JOIN fields f \
         LEFT JOIN student_values sv ON sv.student_id = s.id AND sv.field_id = f.id AND sv.snapshot_id = ? \
         WHERE f.data_type = 'level' \
         ORDER BY s.id, f.id"
    )
    .bind(snapshot_id)
    .fetch_all(db_pool)
    .await
    .map_err(db_err)?;



    let mut student_map: HashMap<i64, StudentLevelRow> = HashMap::new();

    for (s_id, s_name, s_usn, f_id, val_opt) in rows {
        let entry = student_map.entry(s_id).or_insert_with(|| StudentLevelRow {
            student_id: s_id,
            name: s_name,
            usn: s_usn,
            levels: HashMap::new(),
        });

        let level_val = val_opt.and_then(|v| {
            if v.is_empty() || v.to_lowercase() == "null" { None } else { v.parse::<i32>().ok() }
        });

        entry.levels.insert(f_id.to_string(), level_val);

        if let Some(lvl) = level_val {
            if let Some(track) = per_track.get_mut(&f_id) {
                *track.counts.entry(lvl).or_insert(0) += 1;
            }
        }
    }

    let student_level_rows: Vec<StudentLevelRow> = student_map.into_values().collect();

    let mut complete_count = 0;
    let mut gap_count = 0;
    let mut not_started_count = 0;

    for row in &student_level_rows {
        let mut is_complete = true;
        let mut has_gap = false;
        let mut has_any_started = false;

        for (f_id_str, level_val) in &row.levels {
            let f_id = f_id_str.parse::<i64>().unwrap_or(0);
            let target_max = field_max.get(&f_id).copied().unwrap_or(4);

            match level_val {
                Some(v) => {
                    has_any_started = true;
                    if *v < target_max {
                        is_complete = false;
                    }
                    if *v == 0 {
                        has_gap = true;
                    }
                }
                None => {
                    is_complete = false;
                    has_gap = true; // empty means gap
                }
            }
        }

        if !has_any_started && !row.levels.is_empty() {
            // Not started if all are None
            not_started_count += 1;
        } else if is_complete && !row.levels.is_empty() {
            complete_count += 1;
        } else if has_gap {
            gap_count += 1;
        }
    }

    let mut pt_vec: Vec<LevelTrackCount> = per_track.into_values().collect();
    pt_vec.sort_by_key(|t| t.field_id);

    Ok(LevelStats {
        per_track: pt_vec,
        student_level_rows,
        complete_count,
        gap_count,
        not_started_count,
    })
}

// ─── generate_report ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn generate_report(
    config: ReportConfig,
    db: State<'_, SqlitePool>,
) -> Result<String, String> {
    log::info!("generate_report: snapshot={}", config.snapshot_id);
    let db_pool = db.inner();

    if config.student_ids.is_empty() || config.field_ids.is_empty() {
        return Err("Must select at least one student and one field.".into());
    }

    let student_id_list = config.student_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
    let field_id_list = config.field_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");

    let snap: (String, String) = sqlx::query_as("SELECT source_label, synced_at FROM snapshots WHERE id = ?")
        .bind(config.snapshot_id)
        .fetch_optional(db_pool).await.map_err(db_err)?
        .ok_or("Snapshot not found")?;

    let snap_label = snap.0;
    let snap_synced = snap.1;

    let fields: Vec<(i64, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id, COALESCE(display_name, label), data_type FROM fields WHERE id IN ({}) ORDER BY id", field_id_list
    )).fetch_all(db_pool).await.map_err(db_err)?;

    let students: Vec<(i64, String, String)> = sqlx::query_as(&format!(
        "SELECT id, name, roll_number FROM students WHERE id IN ({}) ORDER BY roll_number", student_id_list
    )).fetch_all(db_pool).await.map_err(db_err)?;

    let current_values: Vec<(i64, i64, String)> = sqlx::query_as(&format!(
        "SELECT student_id, field_id, value FROM student_values WHERE snapshot_id = ? AND student_id IN ({}) AND field_id IN ({})",
        student_id_list, field_id_list
    )).bind(config.snapshot_id).fetch_all(db_pool).await.map_err(db_err)?;

    let mut cur_map: HashMap<(i64, i64), String> = HashMap::new();
    for (sid, fid, val) in current_values {
        cur_map.insert((sid, fid), val);
    }

    let mut prev_map: HashMap<(i64, i64), String> = HashMap::new();
    if config.include_progress_notes {
        let prev_snap: Option<i64> = sqlx::query_scalar("SELECT id FROM snapshots WHERE id < ? ORDER BY id DESC LIMIT 1")
            .bind(config.snapshot_id)
            .fetch_optional(db_pool).await.map_err(db_err)?;

        if let Some(pid) = prev_snap {
            let prev_values: Vec<(i64, i64, String)> = sqlx::query_as(&format!(
                "SELECT student_id, field_id, value FROM student_values WHERE snapshot_id = ? AND student_id IN ({}) AND field_id IN ({})",
                student_id_list, field_id_list
            )).bind(pid).fetch_all(db_pool).await.map_err(db_err)?;

            for (sid, fid, val) in prev_values {
                prev_map.insert((sid, fid), val);
            }
        }
    }

    // summary blocks
    let mut summary = String::new();
    let mut num_numeric = 0;
    
    for (fid, label, dtype) in &fields {
        let dtype_str = dtype.as_deref().unwrap_or("text");
        if dtype_str == "score" {
            num_numeric += 1;
            let mut sum: f64 = 0.0;
            let mut count = 0;
            for (sid, _, _) in &students {
                if let Some(val) = cur_map.get(&(*sid, *fid)) {
                    if let Ok(num) = val.parse::<f64>() {
                        sum += num;
                        count += 1;
                    }
                }
            }
            if count > 0 {
                summary.push_str(&format!(
                    "<div class='summary-card'><h3>Average: {}</h3><p>{:.1}</p></div>",
                    label, sum / count as f64
                ));
            }
        } else if dtype_str == "level" || dtype_str == "categorical" {
            let mut counts: HashMap<String, i64> = HashMap::new();
            for (sid, _, _) in &students {
                if let Some(val) = cur_map.get(&(*sid, *fid)) {
                    if !val.is_empty() {
                        *counts.entry(val.clone()).or_insert(0) += 1;
                    }
                }
            }
            if !counts.is_empty() {
                summary.push_str(&format!("<div class='summary-card'><h3>Levels: {}</h3><div style='font-size:13px; margin-top:4px;'>", label));
                let mut levels: Vec<_> = counts.into_iter().collect();
                levels.sort_by(|a, b| a.0.cmp(&b.0));
                for (lvl, count) in levels {
                    summary.push_str(&format!("<div>{}: <b>{}</b></div>", lvl, count));
                }
                summary.push_str("</div></div>");
            }
        }
    }
    
    if num_numeric == 0 && summary.is_empty() {
        summary.push_str("<div class='summary-card'><h3>Averages</h3><p>No numeric metrics</p></div>");
    }

    // build table body
    let mut table_rows = String::new();

    for (sid, name, roll) in &students {
        table_rows.push_str("<tr>");
        table_rows.push_str(&format!(
            "<td><div class='student-name'>{}</div><div class='student-roll'>{}</div></td>",
            name, roll
        ));

        let mut improved = 0;
        let mut regressed = 0;
        let mut unchanged = 0;

        for (fid, _, dtype) in &fields {
            let dtype_str = dtype.as_deref().unwrap_or("text");
            let cur = cur_map.get(&(*sid, *fid));
            let prev = prev_map.get(&(*sid, *fid));
            
            // For link-type fields, show "Submitted" / "Not submitted" instead of raw URL
            let val_str = if dtype_str == "link" {
                match cur {
                    Some(v) if !v.is_empty() => "Submitted".to_string(),
                    _ => "Not submitted".to_string(),
                }
            } else {
                cur.cloned().unwrap_or_else(|| "—".to_string())
            };
            let mut td_class = "";

            // For link fields, colour based on presence
            if dtype_str == "link" {
                td_class = match cur {
                    Some(v) if !v.is_empty() => "bg-up",
                    _ => "bg-down",
                };
            } else if config.include_progress_notes {
                if let (Some(c), Some(p)) = (cur, prev) {
                    if c == p {
                        unchanged += 1;
                    } else if dtype_str == "score" || dtype_str == "level" {
                        if let (Ok(num_c), Ok(num_p)) = (c.parse::<f64>(), p.parse::<f64>()) {
                            if num_c > num_p {
                                improved += 1;
                                td_class = "bg-up";
                            } else if num_c < num_p {
                                regressed += 1;
                                td_class = "bg-down";
                            } else {
                                unchanged += 1;
                            }
                        } else {
                            // parse failed, fallback text change
                            improved += 1;
                            td_class = "bg-up";
                        }
                    } else {
                        // Text changes
                        improved += 1;
                        td_class = "bg-up";
                    }
                } else if cur.is_some() && prev.is_none() {
                    improved += 1;
                    td_class = "bg-new";
                }
            }

            table_rows.push_str(&format!("<td class='{}'>{}</td>", td_class, val_str));
        }
        table_rows.push_str("</tr>");

        if config.include_progress_notes {
            table_rows.push_str(&format!(
                "<tr class='notes'><td colspan='{}'>&uarr; {} improved &nbsp; &darr; {} regressed &nbsp; &rarr; {} unchanged</td></tr>",
                fields.len() + 1, improved, regressed, unchanged
            ));
        }
    }

    let mut ths = String::new();
    for (_, label, _) in &fields {
        ths.push_str(&format!("<th>{}</th>", label));
    }

    let now = chrono::Utc::now().format("%b %d, %Y at %I:%M %p UTC");

    let summary_html = if config.include_summary {
        format!(
            "<div class='summary'>\n  <div class='summary-card'><h3>Selected Students</h3><p>{}</p></div>\n  {}\n</div>",
            students.len(), summary
        )
    } else {
        String::new()
    };

    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  :root {{
    --bg: #ffffff;
    --text: #18181b;
    --text-muted: #71717a;
    --border: #e4e4e7;
    --primary: #f59e0b;
  }}
  body {{ 
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; 
    line-height: 1.5; 
    color: var(--text); 
    max-width: 1000px;
    margin: 0 auto; 
    padding: 32px;
  }}
  header {{ 
    margin-bottom: 24px; 
    padding-bottom: 16px; 
    border-bottom: 2px solid var(--text);
  }}
  h1 {{ 
    margin: 0 0 8px 0; 
    font-size: 24px; 
    letter-spacing: -0.02em;
  }}
  .meta {{ 
    color: var(--text-muted); 
    font-size: 13px;
  }}
  .meta span {{ font-weight: 500; color: var(--text); }}
  
  .summary {{ 
    display: flex; 
    flex-wrap: wrap; 
    gap: 16px; 
    margin-bottom: 32px; 
  }}
  .summary-card {{ 
    background: #fafafa; 
    border: 1px solid var(--border); 
    border-radius: 8px; 
    padding: 16px; 
    flex: 1; 
    min-width: 200px;
  }}
  .summary-card h3 {{ 
    margin: 0 0 8px 0; 
    font-size: 11px; 
    text-transform: uppercase; 
    letter-spacing: 0.05em; 
    color: var(--text-muted);
  }}
  .summary-card p {{ 
    margin: 0; 
    font-size: 24px; 
    font-weight: 600; 
    font-family: monospace;
  }}
  
  table {{ 
    width: 100%; 
    border-collapse: separate; 
    border-spacing: 0; 
    font-size: 14px;
    margin-bottom: 48px;
  }}
  th, td {{ 
    padding: 12px 16px; 
    text-align: left; 
    border-bottom: 1px solid var(--border); 
  }}
  th {{ 
    background: #fafafa;
    font-weight: 600; 
    color: var(--text-muted);
    font-size: 11px; 
    letter-spacing: 0.05em; 
    text-transform: uppercase;
  }}
  td {{ 
    font-family: monospace; 
    font-size: 14px;
  }}
  .student-name {{
    font-family: -apple-system, sans-serif;
    font-weight: 600;
    font-size: 14px;
    margin-bottom: 2px;
  }}
  .student-roll {{
    font-size: 11px;
    color: var(--text-muted);
  }}
  .bg-up {{ background-color: #dcfce7 !important; color: #166534; font-weight: 500; }}
  .bg-down {{ background-color: #fee2e2 !important; color: #991b1b; font-weight: 500; }}
  .bg-new {{ background-color: #fef3c7 !important; color: #92400e; }}
  
  .notes td {{ 
    padding: 6px 16px; 
    font-family: -apple-system, sans-serif; 
    font-size: 11px;
    font-style: italic; 
    border-top: none; 
    color: var(--text-muted);
    background-color: transparent !important;
  }}
  
  .footer {{ 
    margin-top: 64px; 
    font-size: 11px; 
    color: #a1a1aa; 
    text-align: center; 
    border-top: 1px solid var(--border); 
    padding-top: 24px; 
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }}
  
  @media print {{
    body {{ padding: 0; max-width: 100%; }}
    .page-break {{ page-break-after: always; }}
    .summary-card {{ page-break-inside: avoid; }}
    tr {{ page-break-inside: avoid; }}
  }}
</style>
</head>
<body>
  <header>
    <h1>Student Progress Report</h1>
    <div class='meta'>Snapshot: <span>{}</span> | Data Synced: {}</div>
  </header>

  {}

  <table>
    <thead>
      <tr><th>Student</th>{}</tr>
    </thead>
    <tbody>
      {}
    </tbody>
  </table>

  <div class='footer'>Generated by ProgressLens &middot; {}</div>
</body>
</html>"#, snap_label, snap_synced, summary_html, ths, table_rows, now);

    Ok(html)
}

// ─── seed_mock_data ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn seed_mock_data(db: State<'_, SqlitePool>) -> Result<SyncResult, String> {
    log::info!("seed_mock_data: Inserting mock data...");

    let mut tx = db.inner().begin().await.map_err(db_err)?;

    // 0. Create a mock sheet
    sqlx::query("INSERT INTO sheets (url, label) VALUES (?, ?) ON CONFLICT(url) DO UPDATE SET label = excluded.label")
        .bind("mock-sheet-url")
        .bind("Mock Data Sheet")
        .execute(&mut *tx).await.map_err(db_err)?;
        
    let sheet_id: i64 = sqlx::query_scalar("SELECT id FROM sheets WHERE url = 'mock-sheet-url'")
        .fetch_one(&mut *tx).await.map_err(db_err)?;

    // 1. Create fields
    let fields = vec![
        ("math_score", "Math Score", "number"),
        ("science_score", "Science Score", "number"),
        ("english_score", "English Score", "number"),
        ("music_level", "Music Level", "text"),
        ("sports_level", "Sports Level", "text"),
    ];

    for (key, label, dtype) in &fields {
        sqlx::query(
            "INSERT INTO fields (sheet_id, sheet_key, label, data_type) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(sheet_id, sheet_key) DO UPDATE SET label = excluded.label, data_type = excluded.data_type"
        )
        .bind(sheet_id)
        .bind(key)
        .bind(label)
        .bind(dtype)
        .execute(&mut *tx).await.map_err(db_err)?;
    }

    // 2. Insert test students
    let students = vec![
        ("S101", "Alice Smith"),
        ("S102", "Bob Jones"),
        ("S103", "Charlie Brown"),
        ("S104", "Diana Prince"),
        ("S105", "Eve Davis"),
        ("S106", "Frank Miller"),
    ];

    for (roll, name) in &students {
        sqlx::query(
            "INSERT INTO students (roll_number, name) \
             VALUES (?, ?) \
             ON CONFLICT(roll_number) DO UPDATE SET name = excluded.name"
        )
        .bind(roll)
        .bind(name)
        .execute(&mut *tx).await.map_err(db_err)?;
    }

    // Fetch IDs
    let math_id: i64 = sqlx::query_scalar("SELECT id FROM fields WHERE sheet_key = 'math_score'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let science_id: i64 = sqlx::query_scalar("SELECT id FROM fields WHERE sheet_key = 'science_score'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let english_id: i64 = sqlx::query_scalar("SELECT id FROM fields WHERE sheet_key = 'english_score'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let music_id: i64 = sqlx::query_scalar("SELECT id FROM fields WHERE sheet_key = 'music_level'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let sports_id: i64 = sqlx::query_scalar("SELECT id FROM fields WHERE sheet_key = 'sports_level'").fetch_one(&mut *tx).await.map_err(db_err)?;

    let alice_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = 'S101'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let bob_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = 'S102'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let charlie_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = 'S103'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let diana_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = 'S104'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let eve_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = 'S105'").fetch_one(&mut *tx).await.map_err(db_err)?;
    let frank_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = 'S106'").fetch_one(&mut *tx).await.map_err(db_err)?;

    // 3. Insert Snapshots
    
    // Snap 1 (14 days ago)
    let snap1_id: i64 = sqlx::query(
        "INSERT INTO snapshots (sheet_id, source_label, synced_at) VALUES (?, ?, datetime('now', '-14 days')) RETURNING id"
    ).bind(sheet_id).bind("Mock Exam 1").fetch_one(&mut *tx).await.map_err(db_err)?.get(0);

    // Snap 2 (7 days ago)
    let snap2_id: i64 = sqlx::query(
        "INSERT INTO snapshots (sheet_id, source_label, synced_at) VALUES (?, ?, datetime('now', '-7 days')) RETURNING id"
    ).bind(sheet_id).bind("Mock Exam 2").fetch_one(&mut *tx).await.map_err(db_err)?.get(0);

    // Snap 3 (now)
    let snap3_id: i64 = sqlx::query(
        "INSERT INTO snapshots (sheet_id, source_label, synced_at) VALUES (?, ?, datetime('now')) RETURNING id"
    ).bind(sheet_id).bind("Mock Exam 3").fetch_one(&mut *tx).await.map_err(db_err)?.get(0);

    // Values for Snapshot 1
    let snap1_vals = vec![
        (alice_id, math_id, "80"), (alice_id, science_id, "75"), (alice_id, english_id, "85"), (alice_id, music_id, "Level 1"), (alice_id, sports_id, "Level 2"),
        (bob_id, math_id, "65"), (bob_id, science_id, "60"), (bob_id, english_id, "70"), (bob_id, music_id, "Level 1"), (bob_id, sports_id, "Level 1"),
        (charlie_id, math_id, "90"), (charlie_id, science_id, "85"), (charlie_id, english_id, "80"), (charlie_id, music_id, "Level 2"), (charlie_id, sports_id, "Level 1"),
        (diana_id, math_id, "75"), (diana_id, science_id, "80"), (diana_id, english_id, "85"), (diana_id, music_id, "Level 1"), (diana_id, sports_id, "Level 3"),
        (eve_id, math_id, "85"), (eve_id, science_id, "90"), (eve_id, english_id, "80"), (eve_id, music_id, "Level 3"), (eve_id, sports_id, "Level 1"),
        (frank_id, math_id, "70"), (frank_id, science_id, "75"), (frank_id, english_id, "70"), (frank_id, music_id, "Level 1"), (frank_id, sports_id, "Level 1"),
    ];

    // Values for Snapshot 2
    let snap2_vals = vec![
        (alice_id, math_id, "85"), (alice_id, science_id, "80"), (alice_id, english_id, "85"), (alice_id, music_id, "Level 2"), (alice_id, sports_id, "Level 2"),
        (bob_id, math_id, "70"), (bob_id, science_id, "65"), (bob_id, english_id, "75"), (bob_id, music_id, "Level 1"), (bob_id, sports_id, "Level 2"),
        (charlie_id, math_id, "92"), (charlie_id, science_id, "88"), (charlie_id, english_id, "82"), (charlie_id, music_id, "Level 2"), (charlie_id, sports_id, "Level 1"),
        (diana_id, math_id, "75"), (diana_id, science_id, "82"), (diana_id, english_id, "88"), (diana_id, music_id, "Level 1"), (diana_id, sports_id, "Level 3"),
        (eve_id, math_id, "90"), (eve_id, science_id, "95"), (eve_id, english_id, "85"), (eve_id, music_id, "Level 3"), (eve_id, sports_id, "Level 2"),
        // Frank has no scores this time (was sick maybe)
    ];

    // Values for Snapshot 3
    let snap3_vals = vec![
        (alice_id, math_id, "90"), (alice_id, science_id, "85"), (alice_id, english_id, "88"), (alice_id, music_id, "Level 2"), (alice_id, sports_id, "Level 3"),
        (bob_id, math_id, "75"), (bob_id, science_id, "70"), (bob_id, english_id, "80"), (bob_id, music_id, "Level 2"), (bob_id, sports_id, "Level 2"),
        (charlie_id, math_id, "95"), (charlie_id, science_id, "90"), (charlie_id, english_id, "85"), (charlie_id, music_id, "Level 3"), (charlie_id, sports_id, "Level 1"),
        (diana_id, math_id, "80"), (diana_id, science_id, "85"), (diana_id, english_id, "90"), (diana_id, music_id, "Level 2"), (diana_id, sports_id, "Level 3"),
        (eve_id, math_id, "95"), (eve_id, science_id, "98"), (eve_id, english_id, "90"), (eve_id, music_id, "Level 3"), (eve_id, sports_id, "Level 3"),
        (frank_id, math_id, "75"), (frank_id, science_id, "80"), (frank_id, english_id, "75"), (frank_id, music_id, "Level 2"), (frank_id, sports_id, "Level 2"),
    ];

    for (snap_id, vals) in &[(snap1_id, snap1_vals), (snap2_id, snap2_vals), (snap3_id, snap3_vals)] {
        for (sid, fid, val) in vals {
            sqlx::query(
                "INSERT INTO student_values (student_id, field_id, snapshot_id, value) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT(student_id, field_id, snapshot_id) DO UPDATE SET value = excluded.value"
            )
            .bind(sid)
            .bind(fid)
            .bind(snap_id)
            .bind(val)
            .execute(&mut *tx).await.map_err(db_err)?;
        }
    }

    tx.commit().await.map_err(db_err)?;

    Ok(SyncResult {
        sheet_id,
        snapshot_id: snap3_id,
        students_upserted: 6,
        fields_detected: 5,
        source_label: "Mock Exam 3".into(),
        synced_at: chrono::Utc::now().to_rfc3339(),
    })
}

// ─── force_sync ────────────────────────────────────────────────────────────

/// Force-sync all sheets, bypassing hash check.
#[tauri::command]
pub async fn force_sync(
    data_dir: State<'_, PathBuf>,
    db: State<'_, SqlitePool>,
    sync_state: State<'_, std::sync::Arc<SyncState>>,
    app_handle: tauri::AppHandle,
) -> Result<bool, String> {
    use tauri::Emitter;

    log::info!("force_sync: running forced sync...");

    let changed = crate::sync_worker::try_sync_if_changed(
        db.inner(),
        &data_dir,
        &sync_state,
        true, // force = true, bypass hash check
    )
    .await?;

    // Always emit the event on force sync so UI refreshes
    let _ = app_handle.emit("sync:updated", ());

    Ok(changed)
}

// ─── get_sync_status ──────────────────────────────────────────────────────

/// Returns the number of seconds since the last successful auto-sync.
/// Returns None if no auto-sync has occurred yet.
#[tauri::command]
pub async fn get_sync_status(
    sync_state: State<'_, std::sync::Arc<SyncState>>,
) -> Result<Option<u64>, String> {
    let last = sync_state.last_sync_at.lock().await;
    match *last {
        Some(instant) => Ok(Some(instant.elapsed().as_secs())),
        None => Ok(None),
    }
}

// ─── AI Approval Workflow ──────────────────────────────────────────────────
//
// All queries use sqlx::query() (runtime, non-macro). The macro form
// sqlx::query! requires DATABASE_URL at compile time; this project doesn't
// set it, which causes E0282 type-inference errors.

use crate::operations::PendingOperation;


/// Returns all non-expired pending operations for the given sheet, newest first.
#[tauri::command]
pub async fn get_pending_operations(
    sheet_id: i64,
    db: State<'_, SqlitePool>,
) -> Result<Vec<PendingOperation>, String> {
    let ops = sqlx::query_as::<_, PendingOperation>(
        "SELECT id, kind, status, payload_json, preview_json, created_at, expires_at
         FROM agent_operations
         WHERE sheet_id = ? AND status = 'pending' AND datetime('now') < expires_at
         ORDER BY created_at DESC",
    )
    .bind(sheet_id)
    .fetch_all(db.inner())
    .await
    .map_err(db_err)?;

    Ok(ops)
}

/// Approve a pending operation.
///
/// Safety:
/// - Status guard prevents duplicate approvals.
/// - Rust-side expiration check is a second layer beyond the SQL WHERE.
/// - For student_update: integer IDs come from the DB; proposed_value is
///   a sqlx bind parameter — never string-interpolated into SQL.
/// - Everything runs in one SQLite transaction; failures roll back fully.
#[tauri::command]
pub async fn approve_operation(
    operation_id: String,
    db: State<'_, SqlitePool>,
) -> Result<(), String> {
    let mut tx = db.begin().await.map_err(db_err)?;

    // 1. Fetch the operation row
    let maybe_row = sqlx::query(
        "SELECT sheet_id, expected_snapshot_id, kind, status, payload_json, expires_at
         FROM agent_operations WHERE id = ?",
    )
    .bind(&operation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_err)?;

    let row = maybe_row
        .ok_or_else(|| format!("Operation '{}' not found", operation_id))?;

    let status: String = row.get("status");
    if status != "pending" {
        return Err(format!(
            "Cannot approve: status is '{status}' (must be 'pending')"
        ));
    }

    // 2. Rust-side expiration guard
    let expires_at_str: String = row.get("expires_at");
    let expires_at =
        chrono::NaiveDateTime::parse_from_str(&expires_at_str, "%Y-%m-%d %H:%M:%S")
            .map_err(|e| format!("Invalid expiration timestamp: {e}"))?;
    if chrono::Utc::now().naive_utc() >= expires_at {
        sqlx::query("UPDATE agent_operations SET status = 'expired' WHERE id = ?")
            .bind(&operation_id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        tx.commit().await.map_err(db_err)?;
        return Err("Operation has expired and cannot be approved".to_string());
    }

    // 3. Execute the mutation (student_update only)
    let kind: String = row.get("kind");
    if kind == "student_update" {
        let payload_json: String = row.get("payload_json");
        let payload: serde_json::Value =
            serde_json::from_str(&payload_json).map_err(|e| e.to_string())?;

        let student_id = payload
            .get("student_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                "Cannot execute: this operation is missing a student_id. \
                The AI must use search_students first to resolve the exact student, \
                then create a new pending operation with the correct student_id."
                    .to_string()
            })?;
        let field_id = payload
            .get("field_id")
            .and_then(|v| v.as_i64())
            .ok_or("Cannot execute: payload is missing field_id")?;
        let proposed_value = payload
            .get("proposed_value")
            .and_then(|v| v.as_str())
            .ok_or("Cannot execute: payload is missing proposed_value")?
            .to_string();
        let snapshot_id: Option<i64> = row.get("expected_snapshot_id");
        let snapshot_id =
            snapshot_id.ok_or("Cannot execute: expected_snapshot_id is NULL — the target snapshot was likely deleted")?;

        // Pre-flight: verify all three FK targets exist before the INSERT
        // This surfaces a clear error instead of the opaque SQLite code 787.
        let student_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM students WHERE id = ?")
            .bind(student_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
        if student_exists.is_none() {
            return Err(format!("Cannot execute: student with id={student_id} no longer exists in the database"));
        }

        let field_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM fields WHERE id = ?")
            .bind(field_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
        if field_exists.is_none() {
            return Err(format!("Cannot execute: field with id={field_id} no longer exists — field configuration may have changed"));
        }

        let snapshot_exists: Option<i64> = sqlx::query_scalar("SELECT id FROM snapshots WHERE id = ?")
            .bind(snapshot_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db_err)?;
        if snapshot_exists.is_none() {
            return Err(format!("Cannot execute: snapshot {snapshot_id} no longer exists — it may have been pruned. Ask the AI to re-create the operation against the current snapshot."));
        }

        sqlx::query(
            "INSERT INTO student_values (snapshot_id, student_id, field_id, value)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(snapshot_id, student_id, field_id) DO UPDATE SET value = excluded.value",
        )
        .bind(snapshot_id)
        .bind(student_id)
        .bind(field_id)
        .bind(&proposed_value)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;
    }
    // For mentor_note / intervention / other: no DB mutation needed,
    // just mark executed + audit below.

    // 4. Mark the operation executed
    sqlx::query(
        "UPDATE agent_operations SET status = 'executed', executed_at = datetime('now') WHERE id = ?",
    )
    .bind(&operation_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    // 5. Audit event
    sqlx::query(
        "INSERT INTO audit_events(operation_id, event_type, actor, details_json)
         VALUES(?, 'operation_approved', 'user', '{}')",
    )
    .bind(&operation_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;
    Ok(())
}

/// Reject a pending operation.
///
/// Maps to DB status 'cancelled' (existing enum value for REJECTED).
/// Records the rejection reason in audit_events.
#[tauri::command]
pub async fn reject_operation(
    operation_id: String,
    reason: Option<String>,
    db: State<'_, SqlitePool>,
) -> Result<(), String> {
    let mut tx = db.begin().await.map_err(db_err)?;

    let maybe_row = sqlx::query("SELECT status FROM agent_operations WHERE id = ?")
        .bind(&operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

    let row = maybe_row
        .ok_or_else(|| format!("Operation '{}' not found", operation_id))?;

    let status: String = row.get("status");
    if status != "pending" {
        return Err(format!(
            "Cannot reject: status is '{status}' (must be 'pending')"
        ));
    }

    sqlx::query("UPDATE agent_operations SET status = 'cancelled' WHERE id = ?")
        .bind(&operation_id)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    let rejection_reason =
        reason.unwrap_or_else(|| "User rejected without reason".to_string());
    let details = serde_json::json!({ "reason": rejection_reason }).to_string();

    sqlx::query(
        "INSERT INTO audit_events(operation_id, event_type, actor, details_json)
         VALUES(?, 'operation_rejected', 'user', ?)",
    )
    .bind(&operation_id)
    .bind(&details)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;
    Ok(())
}
