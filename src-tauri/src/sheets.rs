// ──────────────────────────────────────────────────────────────
// sheets.rs — Google Sheets API v4 fetch + parse
// ──────────────────────────────────────────────────────────────
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── Public types ──────────────────────────────────────────────────────────

/// Metadata about a single column detected from the header row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMeta {
    /// Original header text, e.g. "Marks Obtained"
    pub label: String,
    /// Normalised key, e.g. "marks_obtained"
    pub field_key: String,
    /// Detected type: "number", "text", or "level"
    pub data_type: String,
}

/// Everything we get from a sheet: column definitions + all data rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SheetData {
    pub headers: Vec<FieldMeta>,
    pub rows: Vec<HashMap<String, String>>,
    /// Human-readable label extracted from the sheet title
    pub source_label: String,
}

// ─── Sheets API v4 response types ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SpreadsheetResponse {
    properties: Option<SpreadsheetProperties>,
    sheets: Option<Vec<SheetInfo>>,
}

#[derive(Debug, Deserialize)]
struct SpreadsheetProperties {
    title: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SheetInfo {
    data: Option<Vec<GridData>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GridData {
    row_data: Option<Vec<RowData>>,
}

#[derive(Debug, Deserialize)]
struct RowData {
    values: Option<Vec<CellData>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CellData {
    formatted_value: Option<String>,
}

// ─── Sheet URL parsing ────────────────────────────────────────────────────

/// Extracts the spreadsheet ID from various Google Sheets URL formats.
pub fn extract_spreadsheet_id(url: &str) -> Result<String, String> {
    // Pattern: /spreadsheets/d/{ID}/...
    let re = Regex::new(r"/spreadsheets/d/([a-zA-Z0-9_-]+)")
        .map_err(|e| e.to_string())?;

    re.captures(url)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
        .ok_or_else(|| format!("Could not extract spreadsheet ID from URL: {}", url))
}

// ─── Fetch sheet data via Sheets API v4 ───────────────────────────────────

/// Fetches the first sheet of the spreadsheet, treating row 1 as headers
/// and rows 2+ as data.
pub async fn fetch_sheet(
    sheet_url: &str,
    access_token: &str,
) -> Result<SheetData, String> {
    let spreadsheet_id = extract_spreadsheet_id(sheet_url)?;

    let api_url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}?includeGridData=true&ranges=A:ZZ",
        spreadsheet_id
    );

    let client = reqwest::Client::new();

    let resp = client
        .get(&api_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Sheets API returned {}: {}", status, body));
    }

    let spreadsheet: SpreadsheetResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Sheets API response: {}", e))?;

    let source_label = spreadsheet
        .properties
        .and_then(|p| p.title)
        .unwrap_or_else(|| "Google Sheet".to_string());

    // Get the first sheet's grid data
    let grid_data = spreadsheet
        .sheets
        .and_then(|mut s| s.pop())
        .and_then(|s| s.data)
        .and_then(|mut d| d.pop())
        .ok_or_else(|| "Spreadsheet contains no data".to_string())?;

    let all_rows = grid_data.row_data.unwrap_or_default();

    if all_rows.is_empty() {
        return Err("Sheet has no rows".to_string());
    }

    // Row 0 = headers
    let raw_headers = extract_row_values(&all_rows[0]);
    if raw_headers.is_empty() {
        return Err("Header row is empty".to_string());
    }

    // Rows 1+ = data
    let data_rows: Vec<Vec<String>> = all_rows[1..]
        .iter()
        .map(extract_row_values)
        .filter(|r| !r.iter().all(|c| c.is_empty())) // skip fully blank rows
        .collect();

    // Auto-detect column types by sampling data
    let headers: Vec<FieldMeta> = raw_headers
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let data_type = detect_column_type(&data_rows, i);
            FieldMeta {
                label: label.clone(),
                field_key: normalise_key(label),
                data_type,
            }
        })
        .collect();

    // Build row maps keyed by field_key
    let rows: Vec<HashMap<String, String>> = data_rows
        .iter()
        .map(|row| {
            headers
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    let value = row.get(i).cloned().unwrap_or_default();
                    (field.field_key.clone(), value)
                })
                .collect()
        })
        .collect();

    Ok(SheetData {
        headers,
        rows,
        source_label: format!("Sheet: {}", source_label),
    })
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn extract_row_values(row: &RowData) -> Vec<String> {
    row.values
        .as_ref()
        .map(|cells| {
            cells
                .iter()
                .map(|c| c.formatted_value.clone().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

/// Turn "Marks Obtained (Term 1)" → "marks_obtained_term_1"
fn normalise_key(label: &str) -> String {
    label
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Detect column type by sampling the first 20 non-empty data values.
fn detect_column_type(data_rows: &[Vec<String>], col_idx: usize) -> String {
    let samples: Vec<&str> = data_rows
        .iter()
        .filter_map(|row| row.get(col_idx).map(|s| s.as_str()))
        .filter(|s| !s.is_empty())
        .take(20)
        .collect();

    if samples.is_empty() {
        return "text".to_string();
    }

    // Check for "Level N" pattern
    let level_re = Regex::new(r"(?i)^level\s*\d+$").unwrap();
    let level_count = samples.iter().filter(|s| level_re.is_match(s)).count();
    if level_count * 2 >= samples.len() {
        return "level".to_string();
    }

    // Check if values are numeric (int or float, allowing %)
    let numeric_re = Regex::new(r"^-?\d+(\.\d+)?%?$").unwrap();
    let numeric_count = samples.iter().filter(|s| numeric_re.is_match(s)).count();
    if numeric_count * 2 >= samples.len() {
        return "number".to_string();
    }

    "text".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_spreadsheet_id() {
        let url = "https://docs.google.com/spreadsheets/d/1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms/edit#gid=0";
        assert_eq!(
            extract_spreadsheet_id(url).unwrap(),
            "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgVE2upms"
        );
    }

    #[test]
    fn test_normalise_key() {
        assert_eq!(normalise_key("Marks Obtained"), "marks_obtained");
        assert_eq!(normalise_key("Term 1 (Final)"), "term_1_final");
        assert_eq!(normalise_key("  spaces  "), "spaces");
    }

    #[test]
    fn test_detect_column_type_numeric() {
        let rows = vec![
            vec!["85".into()],
            vec!["92.5".into()],
            vec!["76".into()],
        ];
        assert_eq!(detect_column_type(&rows, 0), "number");
    }

    #[test]
    fn test_detect_column_type_level() {
        let rows = vec![
            vec!["Level 3".into()],
            vec!["Level 2".into()],
            vec!["level 1".into()],
        ];
        assert_eq!(detect_column_type(&rows, 0), "level");
    }

    #[test]
    fn test_detect_column_type_text() {
        let rows = vec![
            vec!["Alice".into()],
            vec!["Bob".into()],
            vec!["Charlie".into()],
        ];
        assert_eq!(detect_column_type(&rows, 0), "text");
    }
}
