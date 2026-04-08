use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::exporter;
use crate::models::{IndexStats, ProviderSnapshot};
use crate::services::ProviderSnapshotService;

use super::sessions::load_detail;
use super::AppState;

/// Open external URL in browser
#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Result<(), String> {
    app.opener()
        .open_url(&url, None::<String>)
        .map_err(|e| format!("failed to open URL: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_index_stats(state: State<AppState>) -> Result<IndexStats, String> {
    let session_count = state
        .db
        .session_count()
        .map_err(|e| format!("failed to get session count: {e}"))?;

    let db_size_bytes = state.db.db_size_bytes();

    let last_index_time = state
        .db
        .get_meta("last_index_time")
        .map_err(|e| format!("failed to read last_index_time: {e}"))?
        .unwrap_or_default();

    Ok(IndexStats {
        session_count,
        db_size_bytes,
        last_index_time,
    })
}

#[tauri::command]
pub fn rebuild_index(state: State<AppState>) -> Result<usize, String> {
    state.indexer.reindex()
}

#[tauri::command]
pub fn clear_index(state: State<AppState>) -> Result<(), String> {
    state
        .db
        .clear_all()
        .map_err(|e| format!("failed to clear index: {e}"))
}

#[tauri::command]
pub fn get_provider_snapshots(state: State<AppState>) -> Result<Vec<ProviderSnapshot>, String> {
    ProviderSnapshotService::new(&state.db).list()
}

#[tauri::command]
pub fn export_session(
    session_id: String,
    format: String,
    output_path: String,
    state: State<AppState>,
) -> Result<(), String> {
    let detail = load_detail(&session_id, &state.db)?;
    exporter::export(&detail, &format, &output_path)
}

#[tauri::command]
pub async fn export_sessions_batch(
    items: Vec<String>,
    format: String,
    output_path: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        use std::io::{BufWriter, Write};
        use tauri::Emitter;
        let file = std::fs::File::create(&output_path)
            .map_err(|e| format!("failed to create zip file: {e}"))?;
        let mut zip = zip::ZipWriter::new(BufWriter::new(file));
        let options = zip::write::SimpleFileOptions::default();
        let total = items.len();

        for (idx, session_id) in items.iter().enumerate() {
            let _ = app.emit(
                "export-progress",
                serde_json::json!({ "current": idx + 1, "total": total }),
            );
            let detail = load_detail(session_id, &state.db)?;
            let ext = match format.as_str() {
                "json" => "json",
                "markdown" => "md",
                "html" => "html",
                _ => "txt",
            };
            // Append short session ID suffix to prevent filename collisions
            let id_suffix = if session_id.len() > 8 {
                &session_id[..8]
            } else {
                session_id.as_str()
            };
            let filename = format!(
                "{}_{}.{}",
                sanitize_filename(&detail.meta.title),
                id_suffix,
                ext
            );
            let content = match format.as_str() {
                "json" => serde_json::to_string_pretty(&detail)
                    .map_err(|e| format!("failed to serialize session: {e}"))?,
                "markdown" => crate::exporter::markdown::render(&detail),
                "html" => crate::exporter::html::render(&detail),
                _ => String::new(),
            };
            zip.start_file(&filename, options)
                .map_err(|e| format!("failed to write zip entry: {e}"))?;
            zip.write_all(content.as_bytes())
                .map_err(|e| format!("failed to write zip content: {e}"))?;
        }
        zip.finish()
            .map_err(|e| format!("failed to finish zip: {e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .take(100)
        .collect::<String>()
        .trim()
        .to_string()
}
