use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::exporter;
use crate::models::{IndexStats, PricingCatalogStatus, ProviderSnapshot};
use crate::pricing::{
    count_models_dev_models, parse_catalog, parse_models_dev, PRICING_CATALOG_JSON_KEY,
    PRICING_CATALOG_MODEL_COUNT_KEY, PRICING_CATALOG_UPDATED_AT_KEY, PRICING_CATALOG_URL,
};
use crate::services::ProviderSnapshotService;

use super::sessions::load_detail;
use super::AppState;

#[derive(Clone, serde::Serialize)]
struct MaintenanceEventPayload {
    job: &'static str,
    phase: &'static str,
    message: Option<String>,
}

fn emit_maintenance(
    app: &AppHandle,
    job: &'static str,
    phase: &'static str,
    message: Option<String>,
) {
    let _ = app.emit(
        "maintenance-status",
        MaintenanceEventPayload {
            job,
            phase,
            message,
        },
    );
}

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
    let usage_last_refreshed_at = state
        .db
        .get_meta("usage_last_refreshed_at")
        .map_err(|e| format!("failed to read usage_last_refreshed_at: {e}"))?
        .unwrap_or_default();

    Ok(IndexStats {
        session_count,
        db_size_bytes,
        last_index_time,
        usage_last_refreshed_at,
    })
}

#[tauri::command]
pub fn get_pricing_catalog_status(state: State<AppState>) -> Result<PricingCatalogStatus, String> {
    let updated_at = state
        .db
        .get_meta(PRICING_CATALOG_UPDATED_AT_KEY)
        .map_err(|e| format!("failed to read pricing updated_at: {e}"))?;
    let model_count = state
        .db
        .get_meta(PRICING_CATALOG_MODEL_COUNT_KEY)
        .map_err(|e| format!("failed to read pricing model count: {e}"))?
        .and_then(|count| count.parse::<u64>().ok())
        .or_else(|| {
            state
                .db
                .get_meta(PRICING_CATALOG_JSON_KEY)
                .ok()
                .flatten()
                .and_then(|json| parse_catalog(&json).map(|catalog| catalog.len() as u64))
        })
        .unwrap_or(0);

    Ok(PricingCatalogStatus {
        updated_at,
        model_count,
    })
}

#[tauri::command]
pub async fn refresh_pricing_catalog(
    state: State<'_, AppState>,
) -> Result<PricingCatalogStatus, String> {
    let response = reqwest::get(PRICING_CATALOG_URL)
        .await
        .map_err(|e| format!("failed to fetch pricing catalog: {e}"))?;
    let response = response
        .error_for_status()
        .map_err(|e| format!("pricing catalog request failed: {e}"))?;
    let body = response
        .text()
        .await
        .map_err(|e| format!("failed to read pricing catalog body: {e}"))?;
    let model_count =
        count_models_dev_models(&body).ok_or_else(|| "invalid models.dev JSON".to_string())?;
    let catalog = parse_models_dev(&body).ok_or_else(|| "invalid models.dev JSON".to_string())?;
    let body = serde_json::to_string(&catalog)
        .map_err(|e| format!("failed to serialize pricing catalog: {e}"))?;
    let updated_at = chrono::Utc::now().to_rfc3339();

    state
        .db
        .set_meta(PRICING_CATALOG_JSON_KEY, &body)
        .map_err(|e| format!("failed to store pricing catalog: {e}"))?;
    state
        .db
        .set_meta(PRICING_CATALOG_UPDATED_AT_KEY, &updated_at)
        .map_err(|e| format!("failed to store pricing timestamp: {e}"))?;
    state
        .db
        .set_meta(PRICING_CATALOG_MODEL_COUNT_KEY, &model_count.to_string())
        .map_err(|e| format!("failed to store pricing model count: {e}"))?;

    Ok(PricingCatalogStatus {
        updated_at: Some(updated_at),
        model_count,
    })
}

#[tauri::command]
pub fn rebuild_index(state: State<AppState>) -> Result<usize, String> {
    state.indexer.reindex()
}

#[tauri::command]
pub async fn start_rebuild_index(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    use std::sync::atomic::Ordering;

    let state = state.inner().clone();
    if state.maintenance_running.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }

    tokio::spawn(async move {
        emit_maintenance(&app, "rebuild_index", "started", None);
        let result = tokio::task::spawn_blocking({
            let state = state.clone();
            move || state.indexer.reindex()
        })
        .await
        .map_err(|e| format!("task join error: {e}"))
        .and_then(|result| result);

        match result {
            Ok(_) => emit_maintenance(&app, "rebuild_index", "finished", None),
            Err(error) => emit_maintenance(&app, "rebuild_index", "failed", Some(error)),
        }
        state.maintenance_running.store(false, Ordering::SeqCst);
    });

    Ok(true)
}

#[tauri::command]
pub fn clear_index(state: State<AppState>) -> Result<(), String> {
    state
        .db
        .clear_all()
        .map_err(|e| format!("failed to clear index: {e}"))
}

#[tauri::command]
pub async fn start_refresh_usage(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    use std::sync::atomic::Ordering;

    let state = state.inner().clone();
    if state.maintenance_running.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }

    tokio::spawn(async move {
        emit_maintenance(&app, "refresh_usage", "started", None);
        let result = tokio::task::spawn_blocking({
            let state = state.clone();
            move || {
                state
                    .db
                    .clear_usage_stats()
                    .map_err(|e| format!("failed to clear usage stats: {e}"))?;
                state.indexer.reindex().map(|_| ())
            }
        })
        .await
        .map_err(|e| format!("task join error: {e}"))
        .and_then(|result| result);

        match result {
            Ok(_) => emit_maintenance(&app, "refresh_usage", "finished", None),
            Err(error) => emit_maintenance(&app, "refresh_usage", "failed", Some(error)),
        }
        state.maintenance_running.store(false, Ordering::SeqCst);
    });

    Ok(true)
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
