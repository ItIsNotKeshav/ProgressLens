// ──────────────────────────────────────────────────────────────
// lib.rs — Tauri application entry point
// ──────────────────────────────────────────────────────────────
mod auth;
mod commands;
mod db;
mod diff;
mod export;
mod models;
mod sheets;
mod snapshot;
mod sync_worker;
mod webhook;

use commands::{
    authenticate_google, generate_report, get_all_students, get_dashboard_stats, get_level_stats, get_diff,
    get_fields, get_field_setup, get_sheets, get_snapshots, save_field_setup, seed_mock_data, sync_from_sheet, sync_sheet, preview_sheet,
    force_sync, get_sync_status,
};
use export::{export_to_excel, export_to_google_sheet, export_current_view};
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Resolve the OS app-local-data directory
            let data_dir = app
                .path()
                .app_local_data_dir()
                .expect("Could not resolve app local data dir");

            let db_path = data_dir.join("progress_lens.db");
            log::info!("Database path: {:?}", db_path);

            // Initialise DB + run migrations synchronously during setup
            let pool = tauri::async_runtime::block_on(db::init_db(db_path))
                .expect("Failed to initialise SQLite database");

            // Register the pool as managed state so commands can access it
            app.manage(pool.clone());

            // Also register data_dir so commands can find auth.json
            app.manage(data_dir.clone());

            // Create shared sync state
            let sync_state = Arc::new(sync_worker::SyncState::new());
            app.manage(sync_state.clone());

            // Spawn background auto-sync (45-second polling)
            sync_worker::spawn_auto_sync(
                app.handle().clone(),
                pool.clone(),
                data_dir.clone(),
                sync_state.clone(),
            );

            // Spawn webhook listener on localhost:19291
            webhook::spawn_webhook_listener(
                app.handle().clone(),
                pool.clone(),
                data_dir.clone(),
                sync_state.clone(),
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            authenticate_google,
            sync_from_sheet,
            sync_sheet,
            get_diff,
            get_all_students,
            get_dashboard_stats,
            get_level_stats,
            get_sheets,
            get_snapshots,
            get_fields,
            get_field_setup,
            save_field_setup,
            generate_report,
            seed_mock_data,
            preview_sheet,
            force_sync,
            get_sync_status,
            export_to_excel,
            export_to_google_sheet,
            export_current_view,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ProgressLens");
}
