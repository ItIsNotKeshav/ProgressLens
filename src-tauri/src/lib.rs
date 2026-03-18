// ──────────────────────────────────────────────────────────────
// lib.rs — Tauri application entry point
// ──────────────────────────────────────────────────────────────
mod auth;
mod commands;
mod db;
mod diff;
mod models;
mod sheets;
mod snapshot;

use commands::{
    authenticate_google, generate_report, get_all_students, get_dashboard_stats, get_diff,
    get_fields, get_sheets, get_snapshots, seed_mock_data, sync_from_sheet, sync_sheet, preview_sheet,
};
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
            app.manage(pool);

            // Also register data_dir so commands can find auth.json
            app.manage(data_dir);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            authenticate_google,
            sync_from_sheet,
            sync_sheet,
            get_diff,
            get_all_students,
            get_dashboard_stats,
            get_sheets,
            get_snapshots,
            get_fields,
            generate_report,
            seed_mock_data,
            preview_sheet,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ProgressLens");
}
