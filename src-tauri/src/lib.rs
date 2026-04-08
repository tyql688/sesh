pub mod commands;
pub mod db;
mod exporter;
pub mod indexer;
pub mod models;
pub mod provider;
pub mod provider_utils;
pub mod providers;
mod services;
mod terminal;
pub mod trash_state;
mod watcher;

use std::sync::Arc;

/// Test helpers — exposes private functions for integration tests.
#[doc(hidden)]
pub mod exporter_test_helpers {
    pub fn render_tool_detail_pub(tool_name: &str, tool_input: &str) -> String {
        crate::exporter::html::render_tool_detail(tool_name, tool_input)
    }
}

#[doc(hidden)]
pub mod command_test_helpers {
    use crate::commands::{get_resume_command_for_tests, load_session_detail_for_tests};
    use crate::db::Database;
    use crate::models::{ProviderSnapshot, SessionDetail, TrashMeta};
    use crate::services::{ProviderSnapshotService, SessionLifecycleService};

    pub fn get_session_detail(db: &Database, session_id: &str) -> Result<SessionDetail, String> {
        load_session_detail_for_tests(db, session_id)
    }

    pub fn get_provider_snapshots(db: &Database) -> Result<Vec<ProviderSnapshot>, String> {
        ProviderSnapshotService::new(db).list()
    }

    pub fn get_resume_command(db: &Database, session_id: &str) -> Result<String, String> {
        get_resume_command_for_tests(db, session_id)
    }

    pub fn trash_session(db: &Database, session_id: &str) -> Result<(), String> {
        SessionLifecycleService::new(db).trash_session(session_id)
    }

    pub fn list_trash() -> Result<Vec<TrashMeta>, String> {
        SessionLifecycleService::list_trash()
    }

    pub fn restore_session(db: &Database, trash_id: &str) -> Result<(), String> {
        SessionLifecycleService::new(db).restore_session(trash_id)
    }

    pub fn delete_session(db: &Database, session_id: &str) -> Result<(), String> {
        SessionLifecycleService::new(db).purge_session(session_id)
    }
}

use commands::AppState;
use db::Database;
use indexer::Indexer;
use tauri::Manager;

/// Detect and fix inconsistencies left by interrupted trash operations.
/// Called once at app startup, after DB is opened.
fn audit_trash_consistency(db: &db::Database) {
    let Ok(trash_dir) = trash_state::trash_dir() else {
        return;
    };
    let meta_path = trash_state::trash_meta_path(&trash_dir);
    let entries = trash_state::read_trash_meta(&meta_path);
    if entries.is_empty() {
        return;
    }

    for entry in &entries {
        // Auto-fix: session in both trash_meta AND DB → complete interrupted trash
        if db.get_session(&entry.id).ok().flatten().is_some() {
            log::warn!(
                "trash audit: session {} found in both trash and DB — completing interrupted trash",
                entry.id
            );
            if let Err(e) = db.delete_session(&entry.id) {
                log::warn!(
                    "trash audit: failed to delete session {} from DB: {e}",
                    entry.id
                );
            }
        }

        // Log: trash file referenced but missing
        if !entry.trash_file.is_empty() {
            let trash_file_path = trash_dir.join(&entry.trash_file);
            if !trash_file_path.exists() {
                log::warn!(
                    "trash audit: session {} references missing trash file: {}",
                    entry.id,
                    entry.trash_file
                );
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = match dirs::data_local_dir() {
        Some(d) => d.join("cc-session"),
        None => {
            log::error!("failed to resolve local data dir");
            std::process::exit(1);
        }
    };

    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::error!("failed to create data dir: {e}");
        std::process::exit(1);
    }

    let db = match Database::open(&data_dir) {
        Ok(db) => Arc::new(db),
        Err(e) => {
            log::error!("failed to open database: {e}");
            std::process::exit(1);
        }
    };

    audit_trash_consistency(&db);

    let providers = provider::all_runtimes();

    let indexer = Indexer::new(Arc::clone(&db), providers);

    let state = AppState {
        db: Arc::clone(&db),
        indexer,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::reindex,
            commands::reindex_providers,
            commands::sync_sources,
            commands::get_tree,
            commands::get_session_detail,
            commands::get_child_sessions,
            commands::search_sessions,
            commands::rename_session,
            commands::delete_session,
            commands::delete_sessions_batch,
            commands::get_session_count,
            commands::export_session,
            commands::get_index_stats,
            commands::rebuild_index,
            commands::clear_index,
            commands::get_provider_snapshots,
            commands::get_resume_command,
            commands::detect_terminal,
            commands::open_in_terminal,
            commands::resume_session,
            commands::trash_session,
            commands::list_trash,
            commands::restore_session,
            commands::empty_trash,
            commands::permanent_delete_trash,
            commands::export_sessions_batch,
            commands::toggle_favorite,
            commands::list_recent_sessions,
            commands::list_favorites,
            commands::is_favorite,
            commands::read_image_base64,
            commands::open_in_folder,
            commands::open_external,
        ])
        .setup(|app| {
            // On Windows, hide native decorations so the custom titlebar is the only one.
            #[cfg(target_os = "windows")]
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_decorations(false);
            }

            // Provider instances are lightweight (just PathBuf); create a separate
            // set for the watcher since Indexer consumed the first set.
            let watcher_providers = provider::all_runtimes();
            match watcher::start_watcher(app.handle().clone(), &watcher_providers) {
                Ok(fs_watcher) => {
                    app.manage(fs_watcher);
                }
                Err(e) => log::warn!("failed to start file watcher: {e}"),
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            log::error!("failed to run tauri application: {e}");
            std::process::exit(1);
        });
}
