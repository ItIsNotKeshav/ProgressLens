// ──────────────────────────────────────────────────────────────
// export.rs — Excel and Google Sheets export commands
// ──────────────────────────────────────────────────────────────
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::State;

use crate::auth;
use crate::models::ReportConfig;

fn db_err(e: sqlx::Error) -> String {
    e.to_string()
}

/// Helper: gather all the data needed for export from the DB.
/// Returns (snap_label, snap_synced, fields, students, cur_map, prev_map).
pub async fn gather_report_data(
    config: &ReportConfig,
    db: &SqlitePool,
) -> Result<
    (
        String,
        String,
        Vec<(i64, String, Option<String>)>,
        Vec<(i64, String, String)>,
        HashMap<(i64, i64), String>,
        HashMap<(i64, i64), String>,
    ),
    String,
> {
    let student_id_list = config
        .student_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let field_id_list = config
        .field_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let snap: (String, String) = sqlx::query_as(
        "SELECT source_label, synced_at FROM snapshots WHERE id = ?",
    )
    .bind(config.snapshot_id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?
    .ok_or("Snapshot not found")?;

    let snap_label = snap.0;
    let snap_synced = snap.1;

    let fields: Vec<(i64, String, Option<String>)> = sqlx::query_as(&format!(
        "SELECT id, COALESCE(display_name, label), data_type FROM fields WHERE id IN ({}) ORDER BY id",
        field_id_list
    ))
    .fetch_all(db)
    .await
    .map_err(db_err)?;

    let students: Vec<(i64, String, String)> = sqlx::query_as(&format!(
        "SELECT id, name, roll_number FROM students WHERE id IN ({}) ORDER BY roll_number",
        student_id_list
    ))
    .fetch_all(db)
    .await
    .map_err(db_err)?;

    let current_values: Vec<(i64, i64, String)> = sqlx::query_as(&format!(
        "SELECT student_id, field_id, value FROM student_values WHERE snapshot_id = ? AND student_id IN ({}) AND field_id IN ({})",
        student_id_list, field_id_list
    ))
    .bind(config.snapshot_id)
    .fetch_all(db)
    .await
    .map_err(db_err)?;

    let mut cur_map: HashMap<(i64, i64), String> = HashMap::new();
    for (sid, fid, val) in current_values {
        cur_map.insert((sid, fid), val);
    }

    let mut prev_map: HashMap<(i64, i64), String> = HashMap::new();
    if config.include_progress_notes {
        let prev_snap: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM snapshots WHERE id < ? ORDER BY id DESC LIMIT 1",
        )
        .bind(config.snapshot_id)
        .fetch_optional(db)
        .await
        .map_err(db_err)?;

        if let Some(pid) = prev_snap {
            let prev_values: Vec<(i64, i64, String)> = sqlx::query_as(&format!(
                "SELECT student_id, field_id, value FROM student_values WHERE snapshot_id = ? AND student_id IN ({}) AND field_id IN ({})",
                student_id_list, field_id_list
            ))
            .bind(pid)
            .fetch_all(db)
            .await
            .map_err(db_err)?;

            for (sid, fid, val) in prev_values {
                prev_map.insert((sid, fid), val);
            }
        }
    }

    Ok((snap_label, snap_synced, fields, students, cur_map, prev_map))
}

/// Determine cell change status: "improved", "regressed", or "unchanged"
fn cell_change_status(
    cur: Option<&String>,
    prev: Option<&String>,
    dtype: &str,
) -> &'static str {
    match (cur, prev) {
        (Some(c), Some(p)) => {
            if c == p {
                "unchanged"
            } else if dtype == "score" || dtype == "level" {
                if let (Ok(nc), Ok(np)) = (c.parse::<f64>(), p.parse::<f64>()) {
                    if nc > np {
                        "improved"
                    } else if nc < np {
                        "regressed"
                    } else {
                        "unchanged"
                    }
                } else {
                    "improved" // text changed = considered improved
                }
            } else {
                "improved"
            }
        }
        (Some(_), None) => "improved",
        _ => "unchanged",
    }
}

// ─── export_to_excel ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn export_to_excel(
    config: ReportConfig,
    db: State<'_, SqlitePool>,
) -> Result<String, String> {
    use rust_xlsxwriter::*;

    log::info!("export_to_excel: snapshot={}", config.snapshot_id);
    let db_pool = db.inner();

    if config.student_ids.is_empty() || config.field_ids.is_empty() {
        return Err("Must select at least one student and one field.".into());
    }

    let (snap_label, _snap_synced, fields, students, cur_map, prev_map) =
        gather_report_data(&config, db_pool).await?;

    let mut workbook = Workbook::new();

    // ── Formats ────────────────────────────────────────────────────
    let header_format = Format::new()
        .set_bold()
        .set_font_size(11.0)
        .set_background_color(Color::RGB(0xF5F5F5))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xCCCCCC));

    let normal_format = Format::new()
        .set_font_size(11.0)
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xE4E4E7));

    let improved_format = Format::new()
        .set_font_size(11.0)
        .set_background_color(Color::RGB(0xDCFCE7))
        .set_font_color(Color::RGB(0x166534))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xE4E4E7));

    let regressed_format = Format::new()
        .set_font_size(11.0)
        .set_background_color(Color::RGB(0xFEE2E2))
        .set_font_color(Color::RGB(0x991B1B))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xE4E4E7));

    // ── Sheet 1: Report ────────────────────────────────────────────
    let sheet1 = workbook.add_worksheet();
    sheet1.set_name("Report").map_err(|e| e.to_string())?;

    // Headers
    sheet1
        .write_string_with_format(0, 0, "Name", &header_format)
        .map_err(|e| e.to_string())?;
    sheet1
        .write_string_with_format(0, 1, "Roll Number", &header_format)
        .map_err(|e| e.to_string())?;

    for (i, (_fid, label, _dtype)) in fields.iter().enumerate() {
        sheet1
            .write_string_with_format(0, (i + 2) as u16, label, &header_format)
            .map_err(|e| e.to_string())?;
    }

    // Set column widths
    sheet1.set_column_width(0, 25).map_err(|e| e.to_string())?;
    sheet1.set_column_width(1, 18).map_err(|e| e.to_string())?;
    for i in 0..fields.len() {
        let label_len = fields[i].1.len() as f64;
        let width = if label_len > 12.0 { label_len + 4.0 } else { 16.0 };
        sheet1
            .set_column_width((i + 2) as u16, width)
            .map_err(|e| e.to_string())?;
    }

    // Data rows
    for (row_idx, (sid, name, roll)) in students.iter().enumerate() {
        let row = (row_idx + 1) as u32;

        sheet1
            .write_string_with_format(row, 0, name, &normal_format)
            .map_err(|e| e.to_string())?;
        sheet1
            .write_string_with_format(row, 1, roll, &normal_format)
            .map_err(|e| e.to_string())?;

        for (col_idx, (fid, _label, dtype)) in fields.iter().enumerate() {
            let col = (col_idx + 2) as u16;
            let cur = cur_map.get(&(*sid, *fid));
            let prev = prev_map.get(&(*sid, *fid));
            let dtype_str = dtype.as_deref().unwrap_or("text");
            let val_str = cur.cloned().unwrap_or_else(|| "—".to_string());

            let fmt = if config.include_progress_notes {
                match cell_change_status(cur, prev, dtype_str) {
                    "improved" => &improved_format,
                    "regressed" => &regressed_format,
                    _ => &normal_format,
                }
            } else {
                &normal_format
            };

            // Try writing as number for numeric fields
            if (dtype_str == "score" || dtype_str == "level") && !val_str.is_empty() && val_str != "—" {
                if let Ok(num) = val_str.parse::<f64>() {
                    sheet1
                        .write_number_with_format(row, col, num, fmt)
                        .map_err(|e| e.to_string())?;
                    continue;
                }
            }

            sheet1
                .write_string_with_format(row, col, &val_str, fmt)
                .map_err(|e| e.to_string())?;
        }
    }

    // Freeze first row
    sheet1.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;

    // ── Sheet 2: Summary ───────────────────────────────────────────
    let sheet2 = workbook.add_worksheet();
    sheet2.set_name("Summary").map_err(|e| e.to_string())?;

    let title_format = Format::new().set_bold().set_font_size(14.0);
    let section_format = Format::new().set_bold().set_font_size(12.0);

    sheet2
        .write_string_with_format(0, 0, &format!("Progress Report — {}", snap_label), &title_format)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_string(1, 0, &format!("Students: {}", students.len()))
        .map_err(|e| e.to_string())?;

    let mut row: u32 = 3;

    // Averages for score fields
    sheet2
        .write_string_with_format(row, 0, "Score Averages", &section_format)
        .map_err(|e| e.to_string())?;
    row += 1;
    sheet2
        .write_string_with_format(row, 0, "Field", &header_format)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_string_with_format(row, 1, "Average", &header_format)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_string_with_format(row, 2, "Count", &header_format)
        .map_err(|e| e.to_string())?;
    row += 1;

    for (fid, label, dtype) in &fields {
        if dtype.as_deref() != Some("score") {
            continue;
        }
        let mut sum: f64 = 0.0;
        let mut count: u64 = 0;
        for (sid, _, _) in &students {
            if let Some(val) = cur_map.get(&(*sid, *fid)) {
                if let Ok(num) = val.parse::<f64>() {
                    sum += num;
                    count += 1;
                }
            }
        }
        if count > 0 {
            sheet2
                .write_string(row, 0, label)
                .map_err(|e| e.to_string())?;
            sheet2
                .write_number(row, 1, sum / count as f64)
                .map_err(|e| e.to_string())?;
            sheet2
                .write_number(row, 2, count as f64)
                .map_err(|e| e.to_string())?;
            row += 1;
        }
    }

    row += 1;

    // Level distribution
    sheet2
        .write_string_with_format(row, 0, "Level Distribution", &section_format)
        .map_err(|e| e.to_string())?;
    row += 1;
    sheet2
        .write_string_with_format(row, 0, "Field", &header_format)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_string_with_format(row, 1, "Level", &header_format)
        .map_err(|e| e.to_string())?;
    sheet2
        .write_string_with_format(row, 2, "Count", &header_format)
        .map_err(|e| e.to_string())?;
    row += 1;

    for (fid, label, dtype) in &fields {
        if dtype.as_deref() != Some("level") {
            continue;
        }
        let mut level_counts: HashMap<String, i64> = HashMap::new();
        for (sid, _, _) in &students {
            if let Some(val) = cur_map.get(&(*sid, *fid)) {
                if !val.is_empty() {
                    *level_counts.entry(val.clone()).or_insert(0) += 1;
                }
            }
        }
        let mut sorted_levels: Vec<_> = level_counts.into_iter().collect();
        sorted_levels.sort_by(|a, b| a.0.cmp(&b.0));

        for (level, count) in sorted_levels {
            sheet2
                .write_string(row, 0, label)
                .map_err(|e| e.to_string())?;
            sheet2
                .write_string(row, 1, &level)
                .map_err(|e| e.to_string())?;
            sheet2
                .write_number(row, 2, count as f64)
                .map_err(|e| e.to_string())?;
            row += 1;
        }
    }

    sheet2.set_column_width(0, 30).map_err(|e| e.to_string())?;
    sheet2.set_column_width(1, 15).map_err(|e| e.to_string())?;
    sheet2.set_column_width(2, 12).map_err(|e| e.to_string())?;

    // ── Save to Downloads ──────────────────────────────────────────
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads directory".to_string())?;

    let safe_label = snap_label
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect::<String>();

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let filename = format!("ProgressReport_{}_{}.xlsx", safe_label.trim(), date);
    let filepath = downloads_dir.join(&filename);

    workbook
        .save(filepath.to_str().unwrap())
        .map_err(|e| format!("Failed to save xlsx: {}", e))?;

    log::info!("Exported to {}", filepath.display());

    // Open with system default app
    let _ = open::that(filepath.to_str().unwrap());

    Ok(filepath.display().to_string())
}

// ─── export_to_google_sheet ────────────────────────────────────────────────

#[tauri::command]
pub async fn export_to_google_sheet(
    config: ReportConfig,
    data_dir: State<'_, PathBuf>,
    db: State<'_, SqlitePool>,
) -> Result<String, String> {
    log::info!("export_to_google_sheet: snapshot={}", config.snapshot_id);
    let db_pool = db.inner();

    if config.student_ids.is_empty() || config.field_ids.is_empty() {
        return Err("Must select at least one student and one field.".into());
    }

    let (snap_label, _snap_synced, fields, students, cur_map, prev_map) =
        gather_report_data(&config, db_pool).await?;

    let access_token = auth::get_access_token(&data_dir).await?;
    let client = reqwest::Client::new();

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let title = format!("Progress Report — {} — {}", snap_label, date);

    // 1. Create new spreadsheet
    let create_body = serde_json::json!({
        "properties": {
            "title": title
        },
        "sheets": [
            {
                "properties": {
                    "title": "Report",
                    "sheetId": 0
                }
            },
            {
                "properties": {
                    "title": "Summary",
                    "sheetId": 1
                }
            }
        ]
    });

    let create_resp = client
        .post("https://sheets.googleapis.com/v4/spreadsheets")
        .bearer_auth(&access_token)
        .json(&create_body)
        .send()
        .await
        .map_err(|e| format!("Create spreadsheet failed: {}", e))?;

    if !create_resp.status().is_success() {
        let status = create_resp.status();
        let body = create_resp.text().await.unwrap_or_default();
        return Err(format!("Sheets API returned {}: {}", status, body));
    }

    let create_result: serde_json::Value = create_resp
        .json()
        .await
        .map_err(|e| format!("Parse create response: {}", e))?;

    let spreadsheet_id = create_result["spreadsheetId"]
        .as_str()
        .ok_or("Missing spreadsheetId in response")?
        .to_string();

    let spreadsheet_url = format!(
        "https://docs.google.com/spreadsheets/d/{}/edit",
        spreadsheet_id
    );

    // 2. Prepare Report sheet data
    let mut report_rows: Vec<Vec<serde_json::Value>> = Vec::new();

    // Header row
    let mut header = vec![
        serde_json::Value::String("Name".into()),
        serde_json::Value::String("Roll Number".into()),
    ];
    for (_fid, label, _dtype) in &fields {
        header.push(serde_json::Value::String(label.clone()));
    }
    report_rows.push(header);

    // Data rows
    for (sid, name, roll) in &students {
        let mut row = vec![
            serde_json::Value::String(name.clone()),
            serde_json::Value::String(roll.clone()),
        ];
        for (fid, _label, _dtype) in &fields {
            let val = cur_map
                .get(&(*sid, *fid))
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            row.push(serde_json::Value::String(val));
        }
        report_rows.push(row);
    }

    // 3. Prepare Summary sheet data
    let mut summary_rows: Vec<Vec<serde_json::Value>> = Vec::new();
    summary_rows.push(vec![serde_json::Value::String(format!(
        "Progress Report — {}",
        snap_label
    ))]);
    summary_rows.push(vec![serde_json::Value::String(format!(
        "Students: {}",
        students.len()
    ))]);
    summary_rows.push(vec![]);
    summary_rows.push(vec![
        serde_json::Value::String("Field".into()),
        serde_json::Value::String("Average".into()),
        serde_json::Value::String("Count".into()),
    ]);

    for (fid, label, dtype) in &fields {
        if dtype.as_deref() != Some("score") {
            continue;
        }
        let mut sum: f64 = 0.0;
        let mut count: u64 = 0;
        for (sid, _, _) in &students {
            if let Some(val) = cur_map.get(&(*sid, *fid)) {
                if let Ok(num) = val.parse::<f64>() {
                    sum += num;
                    count += 1;
                }
            }
        }
        if count > 0 {
            summary_rows.push(vec![
                serde_json::Value::String(label.clone()),
                serde_json::Value::String(format!("{:.1}", sum / count as f64)),
                serde_json::Value::String(count.to_string()),
            ]);
        }
    }

    // 4. Write data via batchUpdate values
    let batch_values_body = serde_json::json!({
        "valueInputOption": "RAW",
        "data": [
            {
                "range": "Report!A1",
                "values": report_rows
            },
            {
                "range": "Summary!A1",
                "values": summary_rows
            }
        ]
    });

    let values_url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values:batchUpdate",
        spreadsheet_id
    );

    let _values_resp = client
        .post(&values_url)
        .bearer_auth(&access_token)
        .json(&batch_values_body)
        .send()
        .await
        .map_err(|e| format!("Write values failed: {}", e))?;

    // 5. Apply formatting via batchUpdate
    let _total_rows = (students.len() + 1) as i64;
    let total_cols = (fields.len() + 2) as i64;

    let mut requests = vec![
        // Bold header row
        serde_json::json!({
            "repeatCell": {
                "range": {
                    "sheetId": 0,
                    "startRowIndex": 0,
                    "endRowIndex": 1,
                    "startColumnIndex": 0,
                    "endColumnIndex": total_cols
                },
                "cell": {
                    "userEnteredFormat": {
                        "textFormat": { "bold": true },
                        "backgroundColor": { "red": 0.96, "green": 0.96, "blue": 0.96 }
                    }
                },
                "fields": "userEnteredFormat(textFormat,backgroundColor)"
            }
        }),
        // Freeze first row
        serde_json::json!({
            "updateSheetProperties": {
                "properties": {
                    "sheetId": 0,
                    "gridProperties": {
                        "frozenRowCount": 1
                    }
                },
                "fields": "gridProperties.frozenRowCount"
            }
        }),
        // Auto-resize columns
        serde_json::json!({
            "autoResizeDimensions": {
                "dimensions": {
                    "sheetId": 0,
                    "dimension": "COLUMNS",
                    "startIndex": 0,
                    "endIndex": total_cols
                }
            }
        }),
    ];

    // Color cells for improved/regressed
    if config.include_progress_notes {
        for (row_idx, (sid, _, _)) in students.iter().enumerate() {
            for (col_idx, (fid, _, dtype)) in fields.iter().enumerate() {
                let dtype_str = dtype.as_deref().unwrap_or("text");
                let cur = cur_map.get(&(*sid, *fid));
                let prev = prev_map.get(&(*sid, *fid));
                let status = cell_change_status(cur, prev, dtype_str);

                let bg = match status {
                    "improved" => Some(serde_json::json!({ "red": 0.863, "green": 0.988, "blue": 0.906 })),
                    "regressed" => Some(serde_json::json!({ "red": 0.996, "green": 0.886, "blue": 0.886 })),
                    _ => None,
                };

                if let Some(bg_color) = bg {
                    requests.push(serde_json::json!({
                        "repeatCell": {
                            "range": {
                                "sheetId": 0,
                                "startRowIndex": row_idx as i64 + 1,
                                "endRowIndex": row_idx as i64 + 2,
                                "startColumnIndex": col_idx as i64 + 2,
                                "endColumnIndex": col_idx as i64 + 3
                            },
                            "cell": {
                                "userEnteredFormat": {
                                    "backgroundColor": bg_color
                                }
                            },
                            "fields": "userEnteredFormat.backgroundColor"
                        }
                    }));
                }
            }
        }
    }

    let format_body = serde_json::json!({
        "requests": requests
    });

    let format_url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}:batchUpdate",
        spreadsheet_id
    );

    let _ = client
        .post(&format_url)
        .bearer_auth(&access_token)
        .json(&format_body)
        .send()
        .await
        .map_err(|e| format!("Format request failed: {}", e))?;

    // 6. Open in browser
    let _ = open::that(&spreadsheet_url);

    log::info!("Exported to Google Sheets: {}", spreadsheet_url);

    Ok(spreadsheet_url)
}

// ─── export_current_view ───────────────────────────────────────────────────

/// Quick export: takes an explicit list of student IDs + field IDs and
/// writes them to an Excel file.  Used by the "Export current view" feature
/// in the toolbar.
#[tauri::command]
pub async fn export_current_view(
    sheet_id: i64,
    student_ids: Vec<i64>,
    field_ids: Vec<i64>,
    db: State<'_, SqlitePool>,
) -> Result<String, String> {
    // Delegate to export_to_excel with a minimal config (no progress notes).
    let config = ReportConfig {
        snapshot_id: {
            // use latest snapshot for this sheet
            let snap: Option<i64> = sqlx::query_scalar(
                "SELECT id FROM snapshots WHERE sheet_id = ? ORDER BY id DESC LIMIT 1",
            )
            .bind(sheet_id)
            .fetch_optional(db.inner())
            .await
            .map_err(db_err)?;
            snap.ok_or("No snapshots found for this sheet")?
        },
        student_ids,
        field_ids,
        include_progress_notes: false,
        include_summary: false,
    };

    // We can't pass tauri::State to another command directly,
    // so just reuse the core logic inline.
    use rust_xlsxwriter::*;

    let db_pool = db.inner();

    let (snap_label, _snap_synced, fields, students, cur_map, _prev_map) =
        gather_report_data(&config, db_pool).await?;

    let mut workbook = Workbook::new();

    let header_format = Format::new()
        .set_bold()
        .set_font_size(11.0)
        .set_background_color(Color::RGB(0xF5F5F5))
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xCCCCCC));

    let normal_format = Format::new()
        .set_font_size(11.0)
        .set_border(FormatBorder::Thin)
        .set_border_color(Color::RGB(0xE4E4E7));

    let sheet = workbook.add_worksheet();
    sheet.set_name("Export").map_err(|e| e.to_string())?;

    sheet
        .write_string_with_format(0, 0, "Name", &header_format)
        .map_err(|e| e.to_string())?;
    sheet
        .write_string_with_format(0, 1, "Roll Number", &header_format)
        .map_err(|e| e.to_string())?;

    for (i, (_fid, label, _dtype)) in fields.iter().enumerate() {
        sheet
            .write_string_with_format(0, (i + 2) as u16, label, &header_format)
            .map_err(|e| e.to_string())?;
    }

    sheet.set_column_width(0, 25).map_err(|e| e.to_string())?;
    sheet.set_column_width(1, 18).map_err(|e| e.to_string())?;
    for i in 0..fields.len() {
        let label_len = fields[i].1.len() as f64;
        let width = if label_len > 12.0 { label_len + 4.0 } else { 16.0 };
        sheet
            .set_column_width((i + 2) as u16, width)
            .map_err(|e| e.to_string())?;
    }

    for (row_idx, (sid, name, roll)) in students.iter().enumerate() {
        let row = (row_idx + 1) as u32;
        sheet
            .write_string_with_format(row, 0, name, &normal_format)
            .map_err(|e| e.to_string())?;
        sheet
            .write_string_with_format(row, 1, roll, &normal_format)
            .map_err(|e| e.to_string())?;

        for (col_idx, (fid, _label, dtype)) in fields.iter().enumerate() {
            let col = (col_idx + 2) as u16;
            let val_str = cur_map
                .get(&(*sid, *fid))
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            let dtype_str = dtype.as_deref().unwrap_or("text");

            if (dtype_str == "score" || dtype_str == "level") && !val_str.is_empty() && val_str != "—" {
                if let Ok(num) = val_str.parse::<f64>() {
                    sheet
                        .write_number_with_format(row, col, num, &normal_format)
                        .map_err(|e| e.to_string())?;
                    continue;
                }
            }

            sheet
                .write_string_with_format(row, col, &val_str, &normal_format)
                .map_err(|e| e.to_string())?;
        }
    }

    sheet.set_freeze_panes(1, 0).map_err(|e| e.to_string())?;

    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads directory".to_string())?;

    let safe_label = snap_label
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect::<String>();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let filename = format!("ProgressExport_{}_{}.xlsx", safe_label.trim(), date);
    let filepath = downloads_dir.join(&filename);

    workbook
        .save(filepath.to_str().unwrap())
        .map_err(|e| format!("Failed to save xlsx: {}", e))?;

    let _ = open::that(filepath.to_str().unwrap());

    Ok(filepath.display().to_string())
}
