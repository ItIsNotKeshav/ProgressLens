// ──────────────────────────────────────────────────────────────
// sync_worker.rs — Background auto-sync (45s polling) + force sync
// ──────────────────────────────────────────────────────────────
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

use sqlx::SqlitePool;

use crate::auth;
use crate::sheets;
use crate::snapshot;

/// Shared state for sync coordination — stored in Tauri managed state.
pub struct SyncState {
    /// Hash of the last raw API response (per sheet URL)
    pub last_hash: Mutex<u64>,
    /// Timestamp of the last successful sync
    pub last_sync_at: Mutex<Option<Instant>>,
    /// Debounce guard: last time a webhook-triggered sync ran
    pub last_trigger_at: Mutex<Option<Instant>>,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            last_hash: Mutex::new(0),
            last_sync_at: Mutex::new(None),
            last_trigger_at: Mutex::new(None),
        }
    }
}

/// Compute a simple hash of a string (for cheap change detection).
fn hash_string(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// Core sync logic: fetch the sheet, check hash, run pipeline if changed.
/// Returns Ok(true) if data changed and was synced, Ok(false) if no change.
pub async fn try_sync_if_changed(
    db: &SqlitePool,
    data_dir: &std::path::Path,
    sync_state: &SyncState,
    force: bool,
) -> Result<bool, String> {
    // Get all sheets from DB
    let sheet_rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, url FROM sheets ORDER BY id ASC")
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;

    if sheet_rows.is_empty() {
        return Ok(false);
    }

    let access_token = auth::get_access_token(data_dir).await?;
    let mut any_changed = false;

    for (sheet_id, sheet_url) in &sheet_rows {
        // Skip mock sheets
        if sheet_url.starts_with("mock-") {
            continue;
        }

        let raw_body = match sheets::fetch_sheet_raw(sheet_url, &access_token).await {
            Ok(body) => body,
            Err(e) => {
                log::warn!("Auto-sync: failed to fetch sheet {}: {}", sheet_id, e);
                continue;
            }
        };

        let new_hash = hash_string(&raw_body);

        if !force {
            let last = *sync_state.last_hash.lock().await;
            if last != 0 && last == new_hash {
                log::debug!("Auto-sync: sheet {} unchanged (hash match)", sheet_id);
                continue;
            }
        }

        // Hash changed (or force) — run the full pipeline
        log::info!("Auto-sync: sheet {} has changes, syncing...", sheet_id);

        let sheet_data = sheets::parse_sheet_response(&raw_body)?;

        let (roll_key, name_key) = snapshot::detect_identity_columns(&sheet_data);
        let _snapshot_id =
            snapshot::save_snapshot(db, *sheet_id, &sheet_data, &roll_key, &name_key).await?;

        // Update hash
        *sync_state.last_hash.lock().await = new_hash;
        any_changed = true;
    }

    if any_changed {
        *sync_state.last_sync_at.lock().await = Some(Instant::now());
    }

    Ok(any_changed)
}

/// Spawn the 45-second polling background task.
pub fn spawn_auto_sync(
    app_handle: tauri::AppHandle,
    db: SqlitePool,
    data_dir: std::path::PathBuf,
    sync_state: Arc<SyncState>,
) {
    tauri::async_runtime::spawn(async move {
        use tokio::time::{interval, Duration};
        use tauri::Emitter;

        let mut ticker = interval(Duration::from_secs(45));
        // The first tick fires immediately — skip it to let the app settle
        ticker.tick().await;

        loop {
            ticker.tick().await;
            log::debug!("Auto-sync: tick");

            match try_sync_if_changed(&db, &data_dir, &sync_state, false).await {
                Ok(true) => {
                    log::info!("Auto-sync: data changed, emitting sync:updated");
                    let _ = app_handle.emit("sync:updated", ());
                }
                Ok(false) => { /* no change */ }
                Err(e) => {
                    log::warn!("Auto-sync error: {}", e);
                }
            }
        }
    });
}
