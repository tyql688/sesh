use tauri::State;

use crate::db::Database;
use crate::models::{Message, MessageRole, Provider, SessionDetail, SessionMeta};
use crate::provider::SessionProvider;

use super::AppState;

#[tauri::command]
pub async fn reindex(state: State<'_, AppState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || state.indexer.reindex())
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub async fn sync_sources(paths: Vec<String>, state: State<'_, AppState>) -> Result<usize, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut unique_paths = std::collections::HashSet::new();
        let mut synced = 0;

        for path in paths {
            if path.is_empty() || !unique_paths.insert(path.clone()) {
                continue;
            }
            if sync_source_from_path(&path, &state)? {
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
    source_path: String,
    provider: String,
    state: State<AppState>,
) -> Result<SessionDetail, String> {
    load_detail(&session_id, &source_path, &provider, &state.db)
}

#[tauri::command]
pub fn delete_session(
    session_id: String,
    source_path: String,
    state: State<AppState>,
) -> Result<(), String> {
    let path = std::path::Path::new(&source_path);
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| format!("failed to delete file '{source_path}': {e}"))?;
    }

    state
        .db
        .delete_session(&session_id)
        .map_err(|e| format!("failed to delete from db: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn delete_sessions_batch(
    items: Vec<(String, String)>,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut deleted: u32 = 0;
        for (session_id, source_path) in &items {
            let path = std::path::Path::new(source_path);
            if path.exists() {
                std::fs::remove_file(path)
                    .map_err(|e| format!("failed to delete file {source_path}: {e}"))?;
            }
            state
                .db
                .delete_session(session_id)
                .map_err(|e| format!("failed to delete session {session_id} from db: {e}"))?;
            deleted += 1;
        }
        Ok(deleted)
    })
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

pub(crate) fn load_detail(
    session_id: &str,
    source_path: &str,
    provider: &str,
    db: &Database,
) -> Result<SessionDetail, String> {
    let provider_enum = str_to_provider(provider);

    let db_meta = find_meta_from_db(db, session_id);

    let resolved_source_path = if source_path.is_empty() {
        db_meta
            .as_ref()
            .map(|m| m.source_path.clone())
            .unwrap_or_default()
    } else {
        source_path.to_string()
    };

    let messages = load_messages_from_provider(&provider_enum, session_id, &resolved_source_path)?;

    let meta = db_meta.unwrap_or_else(|| {
        build_fallback_meta(session_id, &resolved_source_path, &provider_enum, &messages)
    });

    Ok(SessionDetail { meta, messages })
}

pub(crate) fn sync_source_for_provider(
    provider: Provider,
    source_path: &str,
    db: &Database,
) -> Result<(), String> {
    let provider_impl = make_provider(&provider)
        .ok_or_else(|| "cannot resolve HOME directory — provider unavailable".to_string())?;

    let sessions = provider_impl
        .scan_source(source_path)
        .map_err(|e| format!("failed to scan source: {e}"))?;

    db.sync_source_snapshot(&provider, source_path, &sessions)
        .map_err(|e| format!("failed to sync source snapshot: {e}"))
}

pub(crate) fn sync_source_from_path(source_path: &str, state: &AppState) -> Result<bool, String> {
    let Some(provider) = provider_from_source_path(source_path) else {
        return Ok(false);
    };

    sync_source_for_provider(provider, source_path, &state.db)?;
    Ok(true)
}

fn provider_from_source_path(source_path: &str) -> Option<Provider> {
    let normalized = source_path.replace('\\', "/");

    // cc-mirror check BEFORE claude — cc-mirror paths also contain /projects/
    if normalized.contains("/.cc-mirror/") && normalized.contains("/config/projects/") {
        return Some(Provider::CcMirror);
    }

    if normalized.contains("/.claude/projects/") {
        return Some(Provider::Claude);
    }

    if normalized.contains("/.codex/sessions/") {
        return Some(Provider::Codex);
    }

    if normalized.contains("/.gemini/tmp/") {
        return Some(Provider::Gemini);
    }

    if normalized.contains("/.kimi/sessions/") {
        return Some(Provider::Kimi);
    }

    if normalized.contains("/.cursor/chats/") {
        return Some(Provider::Cursor);
    }

    if normalized.contains("/opencode/opencode.db") {
        return Some(Provider::OpenCode);
    }

    None
}

fn make_provider(provider: &Provider) -> Option<Box<dyn SessionProvider>> {
    crate::provider::make_provider(provider)
}

fn load_messages_from_provider(
    provider: &Provider,
    session_id: &str,
    source_path: &str,
) -> Result<Vec<Message>, String> {
    make_provider(provider)
        .ok_or_else(|| "cannot resolve HOME directory — provider unavailable".to_string())?
        .load_messages(session_id, source_path)
        .map_err(|e| format!("failed to load messages: {e}"))
}

fn find_meta_from_db(db: &Database, session_id: &str) -> Option<SessionMeta> {
    db.get_session(session_id).ok().flatten()
}

fn build_fallback_meta(
    session_id: &str,
    source_path: &str,
    provider: &Provider,
    messages: &[Message],
) -> SessionMeta {
    let title = messages
        .iter()
        .find(|m| m.role == MessageRole::User && !m.content.is_empty())
        .map(|m| {
            if m.content.chars().count() > 100 {
                let mut t: String = m.content.chars().take(100).collect();
                t.push_str("...");
                t
            } else {
                m.content.clone()
            }
        })
        .unwrap_or_else(|| "Untitled".to_string());

    SessionMeta {
        id: session_id.to_string(),
        provider: provider.clone(),
        title,
        project_path: String::new(),
        project_name: String::new(),
        created_at: 0,
        updated_at: 0,
        message_count: messages.len() as u32,
        file_size_bytes: 0,
        source_path: source_path.to_string(),
        is_sidechain: false,
        variant_name: None,
    }
}

fn str_to_provider(s: &str) -> Provider {
    Provider::parse(s).unwrap_or_else(|| {
        eprintln!("warning: unknown provider '{}', falling back to Claude", s);
        Provider::Claude
    })
}

#[tauri::command]
pub fn read_image_base64(path: String) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("image not found: {path}"));
    }

    // Validate path is within allowed directories (home, tmp)
    if let Ok(canonical) = p.canonicalize() {
        let s = canonical.to_string_lossy();
        let home_ok = dirs::home_dir().is_some_and(|h| s.starts_with(&*h.to_string_lossy()));
        let tmp_ok = {
            #[cfg(not(target_os = "windows"))]
            {
                s.starts_with("/tmp/")
                    || s.starts_with("/private/tmp/")
                    || s.starts_with("/var/folders/")
            }
            #[cfg(target_os = "windows")]
            {
                std::env::var("TEMP")
                    .map(|t| s.starts_with(&t))
                    .unwrap_or(false)
                    || std::env::var("TMP")
                        .map(|t| s.starts_with(&t))
                        .unwrap_or(false)
            }
        };
        if !home_ok && !tmp_ok {
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
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err(format!("path not found: {path}"));
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
