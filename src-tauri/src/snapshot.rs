// Snapshot persistence with content-based duplicate prevention.
use sqlx::{SqlitePool, Transaction, Sqlite};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;
use tokio::sync::Mutex;

use crate::sheets::SheetData;

static SNAPSHOT_SAVE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct SnapshotSaveResult {
    pub snapshot_id: i64,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct SnapshotContent {
    fields: BTreeSet<String>,
    students: BTreeMap<String, StudentContent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct StudentContent {
    name: String,
    values: BTreeMap<String, String>,
}

fn incoming_content(data: &SheetData, roll_key: &str, name_key: &str) -> SnapshotContent {
    let fields: BTreeSet<String> = data.headers.iter()
        .filter(|header| header.field_key != roll_key && header.field_key != name_key)
        .map(|header| header.field_key.clone())
        .collect();
    let mut students = BTreeMap::new();
    for row in &data.rows {
        let roll_number = row.get(roll_key).map(String::as_str).unwrap_or("").trim();
        if roll_number.is_empty() { continue; }
        let name = row.get(name_key).cloned().unwrap_or_default();
        let values = fields.iter().map(|key| (key.clone(), row.get(key).cloned().unwrap_or_default())).collect();
        students.insert(roll_number.to_string(), StudentContent { name, values });
    }
    SnapshotContent { fields, students }
}

async fn latest_content(pool: &SqlitePool, sheet_id: i64) -> Result<Option<(i64, SnapshotContent)>, String> {
    let latest: Option<i64> = sqlx::query_scalar("SELECT id FROM snapshots WHERE sheet_id = ? ORDER BY id DESC LIMIT 1")
        .bind(sheet_id).fetch_optional(pool).await.map_err(db_err)?;
    let Some(snapshot_id) = latest else { return Ok(None); };
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT s.roll_number, s.name, f.sheet_key, sv.value FROM student_values sv \
         JOIN students s ON s.id = sv.student_id JOIN fields f ON f.id = sv.field_id \
         WHERE sv.snapshot_id = ? AND f.sheet_id = ? ORDER BY s.roll_number, f.sheet_key"
    ).bind(snapshot_id).bind(sheet_id).fetch_all(pool).await.map_err(db_err)?;
    let mut content = SnapshotContent::default();
    for (roll_number, name, field_key, value) in rows {
        content.fields.insert(field_key.clone());
        let student = content.students.entry(roll_number).or_insert_with(|| StudentContent { name, values: BTreeMap::new() });
        student.values.insert(field_key, value);
    }
    Ok(Some((snapshot_id, content)))
}

/// Save a snapshot only when its canonical content differs from the latest
/// snapshot for the same sheet. Row and column ordering do not affect equality.
pub async fn save_snapshot(
    pool: &SqlitePool,
    sheet_id: i64,
    data: &SheetData,
    roll_number_key: &str,
    name_key: &str,
) -> Result<SnapshotSaveResult, String> {
    // All save paths run in this process. Serializing compare+insert prevents two
    // concurrent sync triggers from both inserting the same content.
    let lock = SNAPSHOT_SAVE_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = lock.lock().await;
    let incoming = incoming_content(data, roll_number_key, name_key);
    if let Some((snapshot_id, current)) = latest_content(pool, sheet_id).await? {
        if current == incoming {
            log::info!("Snapshot skipped for sheet {}: no data changes (latest={})", sheet_id, snapshot_id);
            return Ok(SnapshotSaveResult { snapshot_id, created: false });
        }
    }

    let mut tx = pool.begin().await.map_err(db_err)?;
    let field_map = upsert_fields(&mut tx, sheet_id, data, roll_number_key, name_key).await?;
    let snapshot_id: i64 = sqlx::query_scalar("INSERT INTO snapshots (sheet_id, source_label) VALUES (?, ?) RETURNING id")
        .bind(sheet_id).bind(&data.source_label).fetch_one(&mut *tx).await.map_err(db_err)?;
    let mut students_upserted = 0usize;
    for row in &data.rows {
        let roll_number = row.get(roll_number_key).map(String::as_str).unwrap_or("").trim();
        let name = row.get(name_key).map(String::as_str).unwrap_or("");
        if roll_number.is_empty() { continue; }
        sqlx::query("INSERT INTO students (name, roll_number) VALUES (?, ?) ON CONFLICT(roll_number) DO UPDATE SET name = excluded.name")
            .bind(name).bind(roll_number).execute(&mut *tx).await.map_err(db_err)?;
        let student_id: i64 = sqlx::query_scalar("SELECT id FROM students WHERE roll_number = ?")
            .bind(roll_number).fetch_one(&mut *tx).await.map_err(db_err)?;
        students_upserted += 1;
        for (field_key, field_id) in &field_map {
            let value = row.get(field_key).map(String::as_str).unwrap_or("");
            sqlx::query("INSERT INTO student_values (student_id, field_id, snapshot_id, value) VALUES (?, ?, ?, ?)")
                .bind(student_id).bind(field_id).bind(snapshot_id).bind(value).execute(&mut *tx).await.map_err(db_err)?;
        }
    }
    tx.commit().await.map_err(db_err)?;
    log::info!("Snapshot {} saved: {} students, {} fields", snapshot_id, students_upserted, field_map.len());
    Ok(SnapshotSaveResult { snapshot_id, created: true })
}

async fn upsert_fields(
    tx: &mut Transaction<'_, Sqlite>, sheet_id: i64, data: &SheetData, roll_key: &str, name_key: &str,
) -> Result<HashMap<String, i64>, String> {
    let mut fields = HashMap::new();
    for header in &data.headers {
        if header.field_key == roll_key || header.field_key == name_key { continue; }
        sqlx::query("INSERT INTO fields (sheet_id, sheet_key, label, data_type, is_visible) VALUES (?, ?, ?, ?, 1) ON CONFLICT(sheet_id, sheet_key) DO NOTHING")
            .bind(sheet_id).bind(&header.field_key).bind(&header.label).bind(&header.data_type).execute(&mut **tx).await.map_err(db_err)?;
        let field_id: i64 = sqlx::query_scalar("SELECT id FROM fields WHERE sheet_id = ? AND sheet_key = ?")
            .bind(sheet_id).bind(&header.field_key).fetch_one(&mut **tx).await.map_err(db_err)?;
        fields.insert(header.field_key.clone(), field_id);
    }
    Ok(fields)
}

fn db_err(error: sqlx::Error) -> String { error.to_string() }

/// Identify identity columns by header labels, with first/second-column fallbacks.
pub fn detect_identity_columns(data: &SheetData) -> (String, String) {
    let mut roll_key = String::new();
    let mut name_key = String::new();
    for header in &data.headers {
        let lower = header.label.to_lowercase();
        if roll_key.is_empty() && (lower.contains("roll") || lower.contains("id") || lower.contains("enrollment") || lower.contains("enrolment") || lower.contains("reg")) {
            roll_key = header.field_key.clone();
        }
        if name_key.is_empty() && (lower.contains("name") || lower.contains("student")) && !lower.contains("roll") {
            name_key = header.field_key.clone();
        }
    }
    if roll_key.is_empty() { if let Some(header) = data.headers.first() { roll_key = header.field_key.clone(); } }
    if name_key.is_empty() { if let Some(header) = data.headers.get(1) { name_key = header.field_key.clone(); } }
    (roll_key, name_key)
}

#[cfg(test)]
mod tests {
    use super::{incoming_content, save_snapshot};
    use crate::sheets::{FieldMeta, SheetData};
    use std::collections::HashMap;

    fn data(rows: Vec<Vec<(&str, &str)>>) -> SheetData {
        SheetData {
            headers: vec![
                FieldMeta { label:"Roll".into(), field_key:"roll".into(), data_type:None },
                FieldMeta { label:"Name".into(), field_key:"name".into(), data_type:None },
                FieldMeta { label:"Score".into(), field_key:"score".into(), data_type:Some("score".into()) },
            ],
            rows: rows.into_iter().map(|row| row.into_iter().map(|(key,value)|(key.into(),value.into())).collect::<HashMap<_,_>>()).collect(),
            source_label:"Test".into(),
        }
    }

    #[test]
    fn equality_is_row_order_independent() {
        let a=incoming_content(&data(vec![vec![("roll","2"),("name","B"),("score","80")],vec![("roll","1"),("name","A"),("score","70")]]),"roll","name");
        let b=incoming_content(&data(vec![vec![("roll","1"),("name","A"),("score","70")],vec![("roll","2"),("name","B"),("score","80")]]),"roll","name");
        assert_eq!(a,b);
    }

    #[test]
    fn value_changes_are_detected() {
        let a=incoming_content(&data(vec![vec![("roll","1"),("name","A"),("score","70")]]),"roll","name");
        let b=incoming_content(&data(vec![vec![("roll","1"),("name","A"),("score","71")]]),"roll","name");
        assert_ne!(a,b);
    }

    #[tokio::test]
    async fn unchanged_sync_reuses_latest_snapshot() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
            .connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO sheets(url,label) VALUES('test-url','Test')").execute(&pool).await.unwrap();
        let sheet_id: i64 = sqlx::query_scalar("SELECT id FROM sheets WHERE url='test-url'").fetch_one(&pool).await.unwrap();
        let sheet = data(vec![vec![("roll","1"),("name","A"),("score","70")]]);
        let first = save_snapshot(&pool, sheet_id, &sheet, "roll", "name").await.unwrap();
        let second = save_snapshot(&pool, sheet_id, &sheet, "roll", "name").await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM snapshots WHERE sheet_id=?").bind(sheet_id).fetch_one(&pool).await.unwrap();
        assert!(first.created);
        assert!(!second.created);
        assert_eq!(first.snapshot_id, second.snapshot_id);
        assert_eq!(count, 1);
    }
}
