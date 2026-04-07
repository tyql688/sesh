use std::sync::Mutex;
use tauri::State;

use crate::models::TrashMeta;
use crate::trash_state::{
    add_shared_deletion, atomic_write_json, read_trash_meta, remove_shared_deletion,
    shared_deletions_path, trash_dir, trash_meta_path,
};

use super::session_resolution::load_session_for_mutation;
use super::{sessions::sync_source_for_provider, AppState};

/// Global lock to serialize all trash metadata read-modify-write operations.
static TRASH_META_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
pub fn trash_session(session_id: String, state: State<AppState>) -> Result<(), String> {
    let trash_dir = trash_dir()?;
    let (meta, children) = load_session_for_mutation(&state.db, &session_id)?;

    let now_ts = chrono::Utc::now().timestamp();
    let meta_path = trash_meta_path(&trash_dir);
    let _lock = TRASH_META_LOCK
        .lock()
        .map_err(|_| "trash meta lock poisoned".to_string())?;

    let provider_impl = meta.provider.require_runtime()?;
    let plan = provider_impl.deletion_plan(&meta, &children);
    let provider_key = meta.provider.key();

    let mut entries = read_trash_meta(&meta_path);
    let records = crate::provider::execute_trash(&plan, &meta, provider_key, &trash_dir, now_ts)?;
    entries.extend(records);

    // Track shared deletions for shared-file sessions
    let shared_deletions_path = shared_deletions_path(&trash_dir);
    if plan.file_action == crate::provider::FileAction::Shared {
        add_shared_deletion(
            &shared_deletions_path,
            &meta.id,
            provider_key,
            &meta.source_path,
        )?;
    }

    atomic_write_json(&meta_path, &entries)?;
    state
        .db
        .delete_session(&session_id)
        .map_err(|e| format!("failed to delete from db: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn list_trash() -> Result<Vec<TrashMeta>, String> {
    let trash_dir = trash_dir()?;
    let meta_path = trash_meta_path(&trash_dir);
    let _lock = TRASH_META_LOCK
        .lock()
        .map_err(|_| "trash meta lock poisoned".to_string())?;
    Ok(read_trash_meta(&meta_path))
}

#[tauri::command]
pub fn restore_session(trash_id: String, state: State<AppState>) -> Result<(), String> {
    let trash_dir = trash_dir()?;
    let meta_path = trash_meta_path(&trash_dir);
    let shared_deletions_path = shared_deletions_path(&trash_dir);
    if !meta_path.exists() {
        return Err("No trash metadata found".to_string());
    }

    let lock = TRASH_META_LOCK
        .lock()
        .map_err(|_| "trash meta lock poisoned".to_string())?;

    let entries = read_trash_meta(&meta_path);
    let entry = match entries.iter().find(|e| e.id == trash_id) {
        Some(e) => e.clone(),
        None => {
            // Already restored (e.g. parent restore removed embedded children)
            drop(lock);
            return Ok(());
        }
    };

    // Collect children to also restore alongside this parent.
    let mut child_entries: Vec<TrashMeta> = Vec::new();
    let remaining: Vec<TrashMeta> = entries
        .into_iter()
        .filter(|e| {
            if e.id == trash_id {
                return false;
            }
            // Match children by parent_id (set during execute_trash)
            if e.parent_id.as_deref() == Some(&trash_id) {
                if e.trash_file.is_empty() {
                    // Embedded child (Kimi): just remove from metadata
                    return false;
                }
                // File-based child: collect for restore
                child_entries.push(e.clone());
                return false;
            }
            // Legacy: embedded children without parent_id (same path, empty trash_file).
            // DEPRECATED: new trash operations always set parent_id. This path only
            // fires for entries created before the parent_id field was introduced.
            if e.trash_file.is_empty()
                && !entry.trash_file.is_empty()
                && e.original_path == entry.original_path
                && e.provider == entry.provider
                && e.parent_id.is_none()
            {
                log::debug!(
                    "restore: legacy child match for session {} (provider={}, path={})",
                    e.id,
                    e.provider,
                    e.original_path
                );
                return false;
            }
            true
        })
        .collect();

    // Restore parent
    let provider_impl = runtime_for_trash_entry(&entry);
    let action = provider_impl
        .as_ref()
        .map(|p| p.restore_action(&entry))
        .unwrap_or_else(|| crate::provider::infer_restore_action(&entry));

    let needs_sync = crate::provider::execute_restore(&action, &entry, &trash_dir, &remaining)?;

    // Restore file-based children
    for child in &child_entries {
        let child_action = provider_impl
            .as_ref()
            .map(|p| p.restore_action(child))
            .unwrap_or_else(|| crate::provider::infer_restore_action(child));
        let _ = crate::provider::execute_restore(&child_action, child, &trash_dir, &remaining);
    }

    // For shared deletions, also clean up the tracking file
    if action == crate::provider::RestoreAction::UndoSharedDeletion {
        remove_shared_deletion(&shared_deletions_path, &entry.id, &entry.original_path)?;
    }

    atomic_write_json(&meta_path, &remaining)?;
    drop(lock);

    if needs_sync {
        sync_source(&entry.provider, &entry.original_path, &state)?;
    }

    Ok(())
}

#[tauri::command]
pub fn empty_trash() -> Result<(), String> {
    let trash_dir = trash_dir()?;
    let meta_path = trash_meta_path(&trash_dir);
    let shared_deletions_path = shared_deletions_path(&trash_dir);

    if meta_path.exists() {
        let _lock = TRASH_META_LOCK
            .lock()
            .map_err(|_| "trash meta lock poisoned".to_string())?;
        let entries = read_trash_meta(&meta_path);

        for entry in &entries {
            if entry.trash_file.is_empty() && !entry.original_path.is_empty() {
                purge_shared_trash_entry(entry, &shared_deletions_path)?;
                continue;
            }

            if !entry.trash_file.is_empty() {
                let file = trash_dir.join(&entry.trash_file);
                if file.exists() {
                    let _ = std::fs::remove_file(&file);
                }
            }

            // Also permanently delete session directory from original location.
            // Try both patterns: <file>.jsonl → <file>/ (Claude) and parent dir (Kimi/Cursor).
            if !entry.original_path.is_empty() {
                cleanup_session_dir(&entry.original_path);
            }

            // Provider-specific cleanup (e.g. Cursor store.db)
            cleanup_provider_entry(entry);
        }

        let empty: Vec<TrashMeta> = Vec::new();
        atomic_write_json(&meta_path, &empty)?;
    }

    Ok(())
}

#[tauri::command]
pub fn permanent_delete_trash(trash_id: String) -> Result<(), String> {
    let trash_dir = trash_dir()?;
    let meta_path = trash_meta_path(&trash_dir);
    let shared_deletions_path = shared_deletions_path(&trash_dir);
    if !meta_path.exists() {
        return Err("No trash metadata found".to_string());
    }

    let _lock = TRASH_META_LOCK
        .lock()
        .map_err(|_| "trash meta lock poisoned".to_string())?;
    let entries = read_trash_meta(&meta_path);

    if let Some(entry) = entries.iter().find(|e| e.id == trash_id) {
        if entry.trash_file.is_empty() && !entry.original_path.is_empty() {
            purge_shared_trash_entry(entry, &shared_deletions_path)?;
        }

        if !entry.trash_file.is_empty() {
            // Only delete the actual file if no other entries reference it
            let remaining_after: Vec<&TrashMeta> =
                entries.iter().filter(|e| e.id != trash_id).collect();
            let others_use_file = remaining_after
                .iter()
                .any(|e| e.trash_file == entry.trash_file);

            if !others_use_file {
                let file = trash_dir.join(&entry.trash_file);
                if file.exists() {
                    let _ = std::fs::remove_file(&file);
                }
            }
        }

        // Also permanently delete session directory from original location.
        if !entry.original_path.is_empty() {
            cleanup_session_dir(&entry.original_path);
        }

        // Provider-specific cleanup (e.g. Cursor store.db)
        cleanup_provider_entry(entry);
    }

    let remaining: Vec<TrashMeta> = entries.into_iter().filter(|e| e.id != trash_id).collect();
    atomic_write_json(&meta_path, &remaining)?;

    Ok(())
}

/// Remove session directory from original location.
/// Tries both patterns to cover all providers:
/// - `<file>.jsonl` → `<file>/` (Claude, Codex, CC-Mirror)
/// - `parent()` of file (Kimi, Cursor — session UUID dir contains subagents/, state.json)
///
/// Safety: only `remove_dir_all` on directories that look session-specific
/// (contain subagents/, state.json, wire.jsonl, or context.jsonl).
/// Shared directories like Gemini's `chats/` are NOT removed.
fn cleanup_session_dir(original_path: &str) {
    let original = std::path::Path::new(original_path);
    for candidate in [
        original.with_extension(""),
        original.parent().unwrap_or(original).to_path_buf(),
    ] {
        if !candidate.is_dir() {
            continue;
        }
        if is_session_dir(&candidate) {
            let _ = std::fs::remove_dir_all(&candidate);
        } else {
            // Only remove if empty (safe for shared directories)
            let _ = std::fs::remove_dir(&candidate);
        }
    }
}

/// Check if a directory looks like a session-specific directory (safe to remove_dir_all).
fn is_session_dir(dir: &std::path::Path) -> bool {
    dir.join("subagents").is_dir()
        || dir.join("state.json").is_file()
        || dir.join("wire.jsonl").is_file()
        || dir.join("context.jsonl").is_file()
}

fn runtime_for_trash_entry(entry: &TrashMeta) -> Option<Box<dyn crate::provider::SessionProvider>> {
    crate::models::Provider::parse(&entry.provider).and_then(|provider| provider.build_runtime())
}

fn purge_shared_trash_entry(
    entry: &TrashMeta,
    shared_deletions_path: &std::path::Path,
) -> Result<(), String> {
    if let Some(provider) = runtime_for_trash_entry(entry) {
        if let Err(err) = provider.purge_from_source(&entry.original_path, &entry.id) {
            log::warn!("failed to purge session {} from source: {err}", entry.id);
        }
    }

    add_shared_deletion(
        shared_deletions_path,
        &entry.id,
        &entry.provider,
        &entry.original_path,
    )
}

fn cleanup_provider_entry(entry: &TrashMeta) {
    if let Some(provider) = runtime_for_trash_entry(entry) {
        provider.cleanup_on_permanent_delete(&entry.id);
    }
}

fn sync_source(provider_str: &str, source_path: &str, state: &AppState) -> Result<(), String> {
    let provider = crate::models::Provider::parse_strict(provider_str)?;
    sync_source_for_provider(provider, source_path, &state.db)
}
