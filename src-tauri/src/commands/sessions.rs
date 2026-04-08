use std::path::Path;

use tauri::State;

use crate::db::Database;
use crate::models::{Message, Provider, SessionDetail, SessionMeta};
use crate::services::{load_session_meta, SessionLifecycleService, SourceSyncService};

use super::AppState;

#[tauri::command]
pub async fn reindex(state: State<'_, AppState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.indexer.reindex())
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub async fn reindex_providers(
    providers: Vec<String>,
    state: State<'_, AppState>,
) -> Result<usize, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let filter: Vec<crate::models::Provider> = providers
            .iter()
            .filter_map(|s| crate::models::Provider::parse(s))
            .collect();
        if filter.is_empty() {
            return Ok(0);
        }
        state.indexer.reindex_providers(Some(&filter))
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub async fn sync_sources(paths: Vec<String>, state: State<'_, AppState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let source_sync = SourceSyncService::new(&state.db);
        let mut unique_paths = std::collections::HashSet::new();
        let mut synced = 0;

        for path in paths {
            if path.is_empty() || !unique_paths.insert(path.clone()) {
                continue;
            }
            if source_sync.sync_source_path(&path)? {
                synced += 1;
            }
        }

        Ok(synced)
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub fn get_tree(state: State<AppState>) -> Result<Vec<crate::models::TreeNode>, String> {
    state.indexer.build_tree()
}

#[tauri::command]
pub fn get_session_detail(
    session_id: String,
    state: State<AppState>,
) -> Result<SessionDetail, String> {
    load_detail(&session_id, &state.db)
}

#[tauri::command]
pub fn get_child_sessions(
    parent_id: String,
    state: State<AppState>,
) -> Result<Vec<SessionMeta>, String> {
    state
        .db
        .get_child_sessions(&parent_id)
        .map_err(|e| format!("db error: {e}"))
}

#[tauri::command]
pub fn delete_session(session_id: String, state: State<AppState>) -> Result<(), String> {
    SessionLifecycleService::new(&state.db).purge_session(&session_id)
}

// TODO: return per-item results when frontend uses this command.
// Currently, partial failure stops the loop and already-deleted items are not reported.
#[tauri::command]
pub async fn delete_sessions_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || SessionLifecycleService::new(&state.db).purge_sessions(&items))
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub fn rename_session(
    session_id: String,
    new_title: String,
    state: State<AppState>,
) -> Result<(), String> {
    state
        .db
        .rename_session(&session_id, &new_title)
        .map_err(|e| format!("failed to rename session: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn get_session_count(state: State<AppState>) -> Result<u64, String> {
    state
        .db
        .session_count()
        .map_err(|e| format!("failed to get session count: {e}"))
}

#[tauri::command]
pub fn toggle_favorite(session_id: String, state: State<AppState>) -> Result<bool, String> {
    let is_fav = state
        .db
        .is_favorite(&session_id)
        .map_err(|e| format!("failed to check favorite: {e}"))?;

    if is_fav {
        state
            .db
            .remove_favorite(&session_id)
            .map_err(|e| format!("failed to remove favorite: {e}"))?;
        Ok(false)
    } else {
        state
            .db
            .add_favorite(&session_id)
            .map_err(|e| format!("failed to add favorite: {e}"))?;
        Ok(true)
    }
}

#[tauri::command]
pub fn list_recent_sessions(
    limit: usize,
    state: State<AppState>,
) -> Result<Vec<SessionMeta>, String> {
    state
        .db
        .list_recent_sessions(limit)
        .map_err(|e| format!("failed to list recent sessions: {e}"))
}

#[tauri::command]
pub fn list_favorites(state: State<AppState>) -> Result<Vec<SessionMeta>, String> {
    state
        .db
        .list_favorites()
        .map_err(|e| format!("failed to list favorites: {e}"))
}

#[tauri::command]
pub fn is_favorite(session_id: String, state: State<AppState>) -> Result<bool, String> {
    state
        .db
        .is_favorite(&session_id)
        .map_err(|e| format!("failed to check favorite: {e}"))
}

pub(crate) fn load_detail(session_id: &str, db: &Database) -> Result<SessionDetail, String> {
    let meta = load_session_meta(db, session_id)?;
    let messages = load_messages_from_provider(&meta.provider, session_id, &meta.source_path)?;
    Ok(SessionDetail { meta, messages })
}

fn load_messages_from_provider(
    provider: &Provider,
    session_id: &str,
    source_path: &str,
) -> Result<Vec<Message>, String> {
    provider
        .require_runtime()?
        .load_messages(session_id, source_path)
        .map_err(|e| format!("failed to load messages: {e}"))
}

/// Session images must live under the user home or system temp (same policy as HTML export).
fn read_image_canonical_allowed(canonical: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return tmp_dir_allows_image(canonical);
    };
    if canonical_under_home(canonical, &home) {
        return true;
    }
    tmp_dir_allows_image(canonical)
}

/// Whether `canonical` lies under the user's profile directory.
#[cfg(windows)]
fn canonical_under_home(canonical: &Path, home: &Path) -> bool {
    if canonical.starts_with(home) {
        return true;
    }
    if let Ok(home_canon) = home.canonicalize() {
        if canonical.starts_with(&home_canon) {
            return true;
        }
    }
    // Last resort: compare prefix after stripping Windows verbatim `\\?\` and ignoring case.
    // Covers edge cases where `starts_with` disagrees on prefix form between paths.
    fn lossy_norm(p: &Path) -> String {
        p.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_ascii_lowercase()
    }
    let c = lossy_norm(canonical);
    let h = lossy_norm(home).trim_end_matches('\\').to_string();
    c == h || c.starts_with(&format!("{h}\\"))
}

#[cfg(not(windows))]
fn canonical_under_home(canonical: &Path, home: &Path) -> bool {
    canonical.starts_with(home)
}

#[cfg(not(target_os = "windows"))]
fn tmp_dir_allows_image(canonical: &Path) -> bool {
    let s = canonical.to_string_lossy();
    s.starts_with("/tmp/") || s.starts_with("/private/tmp/") || s.starts_with("/var/folders/")
}

#[cfg(target_os = "windows")]
fn tmp_dir_allows_image(canonical: &Path) -> bool {
    ["TEMP", "TMP"].iter().any(|key| {
        std::env::var(key).ok().is_some_and(|raw| {
            let base = Path::new(raw.trim());
            match base.canonicalize() {
                Ok(c) => canonical.starts_with(&c),
                Err(_) => canonical.starts_with(base),
            }
        })
    })
}

#[tauri::command]
pub fn read_image_base64(path: String) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let path = path.trim().trim_start_matches('\u{feff}').to_string();
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("image not found: {path}"));
    }

    if let Ok(canonical) = p.canonicalize() {
        if !read_image_canonical_allowed(&canonical) {
            log::warn!(
                "read_image_base64 denied (not under home/temp): {}",
                canonical.display()
            );
            return Err(format!("image path not allowed: {path}"));
        }
    }

    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        _ => "image/png",
    };

    let data = std::fs::read(p).map_err(|e| format!("failed to read image {path}: {e}"))?;
    let b64 = STANDARD.encode(&data);
    Ok(format!("data:{mime};base64,{b64}"))
}

#[tauri::command]
pub fn open_in_folder(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("path not found: {path}"));
    }
    // Validate path is under HOME to prevent opening arbitrary system directories
    let canonical = p
        .canonicalize()
        .map_err(|e| format!("failed to resolve path '{path}': {e}"))?;
    let home_ok = dirs::home_dir().is_some_and(|h| canonical.starts_with(&h));
    if !home_ok {
        return Err(format!("path not allowed: {path}"));
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("failed to open: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("failed to open: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("failed to open: {e}"))?;
    }
    Ok(())
}
