// ──────────────────────────────────────────────────────────────
// webhook.rs — Local HTTP server for Apps Script webhook triggers
// ──────────────────────────────────────────────────────────────
use std::sync::Arc;
use std::time::Instant;

use sqlx::SqlitePool;

use crate::sync_worker::SyncState;

/// Spawn the tiny_http server on localhost:19291.
/// On POST /sync-trigger, run a debounced sync (5-second cooldown).
pub fn spawn_webhook_listener(
    app_handle: tauri::AppHandle,
    db: SqlitePool,
    data_dir: std::path::PathBuf,
    sync_state: Arc<SyncState>,
) {
    std::thread::spawn(move || {
        let server = match tiny_http::Server::http("127.0.0.1:19291") {
            Ok(s) => s,
            Err(e) => {
                log::error!("Webhook: failed to bind localhost:19291 — {}", e);
                return;
            }
        };

        log::info!("Webhook listener started on http://127.0.0.1:19291");

        for request in server.incoming_requests() {
            // Only accept POST to /sync-trigger
            let url = request.url().to_string();
            let method = request.method().to_string();

            if method != "POST" || !url.starts_with("/sync-trigger") {
                let resp = tiny_http::Response::from_string("{\"error\":\"not found\"}")
                    .with_status_code(404)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    );
                let _ = request.respond(resp);
                continue;
            }

            // Respond 200 immediately (don't block the caller)
            let resp = tiny_http::Response::from_string("{\"ok\":true}")
                .with_status_code(200)
                .with_header(
                    tiny_http::Header::from_bytes(
                        &b"Content-Type"[..],
                        &b"application/json"[..],
                    )
                    .unwrap(),
                );
            let _ = request.respond(resp);

            // Debounce: skip if last trigger was within 5 seconds
            let sync_state_clone = sync_state.clone();
            let db_clone = db.clone();
            let data_dir_clone = data_dir.clone();
            let app_handle_clone = app_handle.clone();

            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;

                let now = Instant::now();
                {
                    let last = sync_state_clone.last_trigger_at.lock().await;
                    if let Some(prev) = *last {
                        if now.duration_since(prev).as_secs() < 5 {
                            log::debug!("Webhook: debounced (within 5s)");
                            return;
                        }
                    }
                }

                // Update trigger timestamp
                *sync_state_clone.last_trigger_at.lock().await = Some(now);

                log::info!("Webhook: triggered sync");

                match crate::sync_worker::try_sync_if_changed(
                    &db_clone,
                    &data_dir_clone,
                    &sync_state_clone,
                    true, // force — webhook means the user edited, so always sync
                )
                .await
                {
                    Ok(_) => {
                        log::info!("Webhook: sync complete, emitting sync:updated");
                        let _ = app_handle_clone.emit("sync:updated", ());
                    }
                    Err(e) => {
                        log::warn!("Webhook: sync error: {}", e);
                    }
                }
            });
        }
    });
}
