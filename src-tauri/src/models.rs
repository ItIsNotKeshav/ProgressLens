// ──────────────────────────────────────────────────────────────
// models.rs — shared Serde-serializable data types
// ──────────────────────────────────────────────────────────────
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Student {
    pub id: i64,
    pub name: String,
    pub roll_number: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sheet {
    pub id: i64,
    pub url: String,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Field {
    pub id: i64,
    pub sheet_id: Option<i64>,
    pub label: String,
    pub sheet_key: String,
    pub data_type: String,
    pub is_visible: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FieldConfig {
    pub sheet_key: String,
    pub label: String,
    pub data_type: String,
    pub is_visible: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub id: i64,
    pub synced_at: String,
    pub source_label: String,
}

/// Flat student row suitable for table rendering
#[derive(Debug, Serialize, Deserialize)]
pub struct StudentRow {
    pub id: i64,
    pub name: String,
    pub roll_number: String,
    pub values: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AvgScore {
    pub field_label: String,
    pub avg: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LevelCount {
    pub field_label: String,
    pub level: String,
    pub count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecentChange {
    pub student_name: String,
    pub field_label: String,
    pub old_val: String,
    pub new_val: String,
    pub synced_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopPerformer {
    pub name: String,
    pub score: f64,
    pub field_label: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_students: i64,
    pub active_this_week: i64,
    pub avg_score_per_field: Vec<AvgScore>,
    pub level_distribution: Vec<LevelCount>,
    pub recent_changes: Vec<RecentChange>,
    pub top_performers: Vec<TopPerformer>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResult {
    pub sheet_id: i64,
    pub snapshot_id: i64,
    pub students_upserted: usize,
    pub fields_detected: usize,
    pub source_label: String,
    pub synced_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FieldDiff {
    pub field_key: String,
    pub label: String,
    pub value_a: Option<String>,
    pub value_b: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StudentDiffStatus {
    Unchanged,
    Changed,
    Added,
    Removed,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StudentDiff {
    pub student_id: i64,
    pub name: String,
    pub roll_number: String,
    pub status: StudentDiffStatus,
    pub field_diffs: Vec<FieldDiff>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffResult {
    pub snapshot_a: i64,
    pub snapshot_b: i64,
    pub total_changed: usize,
    pub total_added: usize,
    pub total_removed: usize,
    pub diffs: Vec<StudentDiff>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportConfig {
    pub snapshot_id: i64,
    pub student_ids: Vec<i64>,
    pub field_ids: Vec<i64>,
    pub include_progress_notes: bool,
    #[serde(default)]
    pub include_summary: bool,
}
