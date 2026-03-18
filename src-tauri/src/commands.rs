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
    AvgScore, DashboardStats, DiffResult, Field, FieldConfig, LevelCount, RecentChange, ReportConfig, Snapshot,
    StudentRow, SyncResult, TopPerformer,
};
use crate::sheets;
use crate::snapshot;

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
    let snapshot_id =
        snapshot::save_snapshot(db_pool, sheet_id, &sheet_data, &roll_key, &name_key).await?;

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
    let snapshot_id =
        snapshot::save_snapshot(db_pool, sheet_id,  &sheet_data, &roll_key, &name_key).await?;

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
    let rows: Vec<(i64, Option<i64>, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, sheet_id, label, sheet_key, data_type, is_visible FROM fields ORDER BY id"
    )
        .fetch_all(db.inner())
        .await
        .map_err(db_err)?;
    
    Ok(rows.into_iter().map(|(id, sheet_id, label, sheet_key, data_type, is_visible)| Field {
        id, sheet_id, label, sheet_key, data_type, is_visible
    }).collect())
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
        "SELECT f.label, COALESCE(AVG(CAST(sv.value AS REAL)), 0.0) \
         FROM student_values sv JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND f.data_type = 'number' AND sv.value != '' \
         GROUP BY f.id"
    )
    .bind(snap_id)
    .fetch_all(db_pool)
    .await
    .unwrap_or_default();
    
    let avg_score_per_field = avg_rows.into_iter().map(|(field_label, avg)| AvgScore { field_label, avg }).collect();

    let level_rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT f.label, sv.value, COUNT(sv.student_id) \
         FROM student_values sv JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND (LOWER(f.label) LIKE '%level%' OR LOWER(sv.value) LIKE '%level%') AND sv.value != '' \
         GROUP BY f.id, sv.value"
    )
    .bind(snap_id)
    .fetch_all(db_pool)
    .await
    .unwrap_or_default();

    let level_distribution = level_rows.into_iter().map(|(field_label, level, count)| LevelCount { field_label, level, count }).collect();

    let top_rows: Vec<(String, f64, String)> = sqlx::query_as(
        "SELECT s.name, CAST(sv.value AS REAL), f.label \
         FROM student_values sv \
         JOIN students s ON s.id = sv.student_id \
         JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND f.data_type = 'number' AND sv.value != '' \
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

    let fields: Vec<(i64, String, String)> = sqlx::query_as(&format!(
        "SELECT id, label, data_type FROM fields WHERE id IN ({}) ORDER BY id", field_id_list
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
        if dtype == "number" {
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
        } else if dtype == "text" && label.to_lowercase().contains("level") {
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
            let cur = cur_map.get(&(*sid, *fid));
            let prev = prev_map.get(&(*sid, *fid));
            
            // For link-type fields, show "Submitted" / "Not submitted" instead of raw URL
            let val_str = if dtype == "link" {
                match cur {
                    Some(v) if !v.is_empty() => "Submitted".to_string(),
                    _ => "Not submitted".to_string(),
                }
            } else {
                cur.cloned().unwrap_or_else(|| "—".to_string())
            };
            let mut td_class = "";

            // For link fields, colour based on presence
            if dtype == "link" {
                td_class = match cur {
                    Some(v) if !v.is_empty() => "bg-up",
                    _ => "bg-down",
                };
            } else if config.include_progress_notes {
                if let (Some(c), Some(p)) = (cur, prev) {
                    if c == p {
                        unchanged += 1;
                    } else if dtype == "number" {
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
