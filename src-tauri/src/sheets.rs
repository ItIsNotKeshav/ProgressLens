// ──────────────────────────────────────────────────────────────
// sheets.rs — Google Sheets API v4 fetch + parse
// ──────────────────────────────────────────────────────────────
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;

// ─── Public types ──────────────────────────────────────────────────────────

/// Metadata about a single column detected from the header row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMeta {
    /// Original header text, e.g. "Marks Obtained"
    pub label: String,
    /// Normalised key, e.g. "marks_obtained"
    pub field_key: String,
    /// Detected type, will be populated on first sync classification
    pub data_type: Option<String>,
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
/// Fetch the raw JSON response from the Sheets API (used for hash-based change detection).
pub async fn fetch_sheet_raw(
    sheet_url: &str,
    access_token: &str,
) -> Result<String, String> {
    let spreadsheet_id = extract_spreadsheet_id(sheet_url)?;
    let api_url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}?includeGridData=true&ranges=A:ZZ&alt=json&prettyPrint=false",
        spreadsheet_id
    );

    let client = reqwest::Client::new();

    let resp = client
        .get(&api_url)
        .bearer_auth(access_token)
        .header("Cache-Control", "no-cache")
        .header("Pragma", "no-cache")
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Sheets API returned {}: {}", status, body));
    }

    resp.text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))
}

/// Parse a raw Sheets API JSON response into SheetData.
/// This is the shared parsing pipeline used by both fetch_sheet and the sync worker.
pub fn parse_sheet_response(raw_body: &str) -> Result<SheetData, String> {
    let spreadsheet: SpreadsheetResponse = serde_json::from_str(raw_body)
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

    // Auto-detect column types by sampling data and skip empty headers
    let mut headers: Vec<FieldMeta> = Vec::new();
    let mut header_indices: Vec<usize> = Vec::new(); // keep track of original column indices

    for (i, label) in raw_headers.iter().enumerate() {
        let field_key = normalise_key(label);
        if field_key.is_empty() {
            continue; // Ignore blank columns
        }
        let data_type = detect_column_type(&data_rows, i);
        headers.push(FieldMeta {
            label: label.clone(),
            field_key,
            data_type,
        });
        header_indices.push(i);
    }

    // Build row maps keyed by field_key
    let rows: Vec<HashMap<String, String>> = data_rows
        .iter()
        .map(|row| {
            headers
                .iter()
                .zip(&header_indices)
                .map(|(field, &col_idx)| {
                    let value = row.get(col_idx).cloned().unwrap_or_default();
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

pub async fn fetch_sheet(
    sheet_url: &str,
    access_token: &str,
) -> Result<SheetData, String> {
    let raw_body = fetch_sheet_raw(sheet_url, access_token).await?;
    parse_sheet_response(&raw_body)
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

/// old detection logic disabled, now returning None
fn detect_column_type(_data_rows: &[Vec<String>], _col_idx: usize) -> Option<String> {
    None
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

}
