use std::sync::Mutex;
use tauri::State;

use crate::models::TrashMeta;
use crate::trash_state::{
    add_shared_deletion, atomic_write_json, read_trash_meta, remove_shared_deletion,
    shared_deletions_path, trash_dir, trash_meta_path,
};

use super::{sessions::sync_source_for_provider, AppState};

/// Global lock to serialize all trash metadata read-modify-write operations.
static TRASH_META_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
pub fn trash_session(
    session_id: String,
    source_path: String,
    provider: String,
    title: String,
    state: State<AppState>,
) -> Result<(), String> {
    let trash_dir = trash_dir()?;

    // Resolve missing fields from DB
    let db_meta = state.db.get_session(&session_id).ok().flatten();

    let resolved_path = if source_path.is_empty() {
        db_meta
            .as_ref()
            .map(|s| s.source_path.clone())
            .unwrap_or_default()
    } else {
        source_path
    };

    let resolved_provider = if provider.is_empty() {
        db_meta.as_ref().map_or_else(
            || "claude".to_string(),
            |s| crate::db::provider_to_str_pub(&s.provider).to_string(),
        )
    } else {
        provider
    };

    let resolved_title = if title.is_empty() {
        db_meta
            .as_ref()
            .map(|s| s.title.clone())
            .unwrap_or_default()
    } else {
        title
    };

    let resolved_project = db_meta
        .as_ref()
        .map(|s| s.project_name.clone())
        .unwrap_or_default();

    let now_ts = chrono::Utc::now().timestamp();
    let meta_path = trash_meta_path(&trash_dir);
    let _lock = TRASH_META_LOCK
        .lock()
        .map_err(|_| "trash meta lock poisoned".to_string())?;

    let provider_enum = crate::models::Provider::parse(&resolved_provider)
        .ok_or_else(|| format!("unknown provider: {}", resolved_provider))?;
    let provider_impl = crate::provider::make_provider(&provider_enum)
        .ok_or_else(|| "cannot resolve HOME directory — provider unavailable".to_string())?;

    let meta = db_meta.unwrap_or_else(|| crate::models::SessionMeta {
        id: session_id.clone(),
        provider: provider_enum.clone(),
        title: resolved_title.clone(),
        project_path: String::new(),
        project_name: resolved_project.clone(),
        created_at: 0,
        updated_at: 0,
        message_count: 0,
        file_size_bytes: 0,
        source_path: resolved_path.clone(),
        is_sidechain: false,
        variant_name: None,
        model: None,
        cc_version: None,
        git_branch: None,
        parent_id: None,
    });

    let children = state.db.get_child_sessions(&session_id).unwrap_or_default();
    let plan = provider_impl.deletion_plan(&meta, &children);
    let provider_key = crate::db::provider_to_str_pub(&provider_enum);

    let mut entries = read_trash_meta(&meta_path);
    let records = crate::provider::execute_trash(&plan, &meta, provider_key, &trash_dir, now_ts)?;
    entries.extend(records);

    // Track shared deletions for shared-file sessions
    let shared_deletions_path = shared_deletions_path(&trash_dir);
    if plan.file_action == crate::provider::FileAction::Shared {
        add_shared_deletion(
            &shared_deletions_path,
            &meta.id,
            &resolved_provider,
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

    // Remove the entry itself AND any embedded children (same original_path, empty trash_file)
    let remaining: Vec<TrashMeta> = entries
        .into_iter()
        .filter(|e| {
            if e.id == trash_id {
                return false;
            }
            // Embedded children share the same original_path and have no trash file
            if e.trash_file.is_empty()
                && !entry.trash_file.is_empty()
                && e.original_path == entry.original_path
                && e.provider == entry.provider
            {
                return false;
            }
            true
        })
        .collect();

    // Provider decides how to restore
    let provider_enum = crate::models::Provider::parse(&entry.provider);
    let provider_impl = provider_enum
        .as_ref()
        .and_then(crate::provider::make_provider);
    let action = provider_impl
        .as_ref()
        .map(|p| p.restore_action(&entry))
        .unwrap_or_else(|| crate::provider::infer_restore_action(&entry));

    let needs_sync = crate::provider::execute_restore(&action, &entry, &trash_dir, &remaining)?;

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
                if let Some(p) = crate::models::Provider::parse(&entry.provider)
                    .and_then(|p| crate::provider::make_provider(&p))
                {
                    if let Err(e) = p.purge_from_source(&entry.original_path, &entry.id) {
                        log::warn!("failed to purge session {} from source: {e}", entry.id);
                    }
                }
                add_shared_deletion(
                    &shared_deletions_path,
                    &entry.id,
                    &entry.provider,
                    &entry.original_path,
                )?;
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
            if let Some(p) = crate::models::Provider::parse(&entry.provider)
                .and_then(|p| crate::provider::make_provider(&p))
            {
                p.cleanup_on_permanent_delete(&entry.id);
            }
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
            if let Some(p) = crate::models::Provider::parse(&entry.provider)
                .and_then(|p| crate::provider::make_provider(&p))
            {
                let _ = p.purge_from_source(&entry.original_path, &entry.id);
            }
            add_shared_deletion(
                &shared_deletions_path,
                &entry.id,
                &entry.provider,
                &entry.original_path,
            )?;
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
        if let Some(p) = crate::models::Provider::parse(&entry.provider)
            .and_then(|p| crate::provider::make_provider(&p))
        {
            p.cleanup_on_permanent_delete(&entry.id);
        }
    }

    let remaining: Vec<TrashMeta> = entries.into_iter().filter(|e| e.id != trash_id).collect();
    atomic_write_json(&meta_path, &remaining)?;

    Ok(())
}

/// Remove session directory from original location.
/// Tries both patterns to cover all providers:
/// - `<file>.jsonl` → `<file>/` (Claude, Codex, CC-Mirror)
/// - `parent()` of file (Kimi, Cursor)
fn cleanup_session_dir(original_path: &str) {
    let original = std::path::Path::new(original_path);
    for candidate in [
        original.with_extension(""),
        original.parent().unwrap_or(original).to_path_buf(),
    ] {
        if candidate.is_dir() {
            let _ = std::fs::remove_dir_all(&candidate);
        }
    }
}

fn sync_source(provider_str: &str, source_path: &str, state: &AppState) -> Result<(), String> {
    let provider = crate::models::Provider::parse(provider_str)
        .ok_or_else(|| format!("unsupported provider: {provider_str}"))?;
    sync_source_for_provider(provider, source_path, &state.db)
}
