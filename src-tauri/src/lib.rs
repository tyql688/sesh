mod commands;
mod db;
mod exporter;
mod indexer;
pub mod models;
pub mod provider;
pub mod provider_utils;
pub mod providers;
mod terminal;
mod trash_state;
mod watcher;

use std::sync::Arc;

use commands::AppState;
use db::Database;
use indexer::Indexer;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = match dirs::data_local_dir() {
        Some(d) => d.join("cc-session"),
        None => {
            eprintln!("fatal: failed to resolve local data dir");
            std::process::exit(1);
        }
    };

    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        eprintln!("fatal: failed to create data dir: {e}");
        std::process::exit(1);
    }

    let db = match Database::open(&data_dir) {
        Ok(db) => Arc::new(db),
        Err(e) => {
            eprintln!("fatal: failed to open database: {e}");
            std::process::exit(1);
        }
    };

    let providers = provider::all_providers();

    let indexer = Indexer::new(Arc::clone(&db), providers);

    let state = AppState {
        db: Arc::clone(&db),
        indexer,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::reindex,
            commands::sync_sources,
            commands::get_tree,
            commands::get_session_detail,
            commands::search_sessions,
            commands::rename_session,
            commands::delete_session,
            commands::delete_sessions_batch,
            commands::get_session_count,
            commands::export_session,
            commands::get_index_stats,
            commands::rebuild_index,
            commands::clear_index,
            commands::get_provider_paths,
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
        ])
        .setup(|app| {
            // Provider instances are lightweight (just PathBuf); create a separate
            // set for the watcher since Indexer consumed the first set.
            let watcher_providers = provider::all_providers();
            match watcher::start_watcher(app.handle().clone(), &watcher_providers) {
                Ok(fs_watcher) => {
                    app.manage(fs_watcher);
                }
                Err(e) => eprintln!("warning: failed to start file watcher: {e}"),
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            eprintln!("fatal: failed to run tauri application: {e}");
            std::process::exit(1);
        });
}
