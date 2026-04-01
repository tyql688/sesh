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

/// Check if a source_path is a multi-session file that should never be moved.
/// Gemini's logs.json contains multiple sessions in one file.
/// Claude/Codex each use one file per session, safe to move.
fn is_shared_file(source_path: &str) -> bool {
    // Gemini logs.json is always shared (multiple sessions per file)
    source_path.ends_with("/logs.json")
}

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

    // Check if this is a shared file (e.g. Gemini logs.json with multiple sessions)
    let shared = !resolved_path.is_empty() && is_shared_file(&resolved_path);

    if shared {
        // SHARED FILE: don't move the file, just remove from DB and record in trash meta.
        // The file stays on disk so other sessions from same file still work.
        let mut entries = read_trash_meta(&meta_path);
        entries.push(TrashMeta {
            id: session_id.clone(),
            provider: resolved_provider,
            title: resolved_title,
            original_path: resolved_path,
            trashed_at: now_ts,
            trash_file: String::new(), // empty = soft-delete, no file moved
            project_name: resolved_project,
        });
        atomic_write_json(&meta_path, &entries)?;

        state
            .db
            .delete_session(&session_id)
            .map_err(|e| format!("failed to delete from db: {e}"))?;
        return Ok(());
    }

    // DEDICATED FILE: move file to trash directory
    let src = std::path::Path::new(&resolved_path);

    if !src.exists() || resolved_path.is_empty() {
        // File already gone, just remove from DB and record
        let mut entries = read_trash_meta(&meta_path);
        entries.push(TrashMeta {
            id: session_id.clone(),
            provider: resolved_provider.clone(),
            title: resolved_title,
            original_path: resolved_path,
            trashed_at: now_ts,
            trash_file: String::new(),
            project_name: resolved_project.clone(),
        });

        // Trash child session files that still exist on disk
        trash_children(&session_id, &resolved_provider, &resolved_project, now_ts, &trash_dir, &mut entries, &state);

        atomic_write_json(&meta_path, &entries)?;

        state
            .db
            .delete_session(&session_id)
            .map_err(|e| format!("failed to delete from db: {e}"))?;
        return Ok(());
    }

    let base_name = src.file_name().map_or_else(
        || format!("{session_id}.jsonl"),
        |f| f.to_string_lossy().to_string(),
    );
    // Sanitize: strip path separators to prevent directory traversal
    let base_name = base_name.replace(['/', '\\'], "_");
    let file_name = if let Some(dot_pos) = base_name.rfind('.') {
        format!(
            "{}_{}{}",
            &base_name[..dot_pos],
            now_ts,
            &base_name[dot_pos..]
        )
    } else {
        format!("{base_name}_{now_ts}")
    };

    let dest = trash_dir.join(&file_name);
    std::fs::rename(src, &dest)
        .or_else(|_| std::fs::copy(src, &dest).and_then(|_| std::fs::remove_file(src)))
        .map_err(|e| format!("failed to move file to trash: {e}"))?;

    let mut entries = read_trash_meta(&meta_path);
    entries.push(TrashMeta {
        id: session_id.clone(),
        provider: resolved_provider.clone(),
        title: resolved_title,
        original_path: resolved_path,
        trashed_at: now_ts,
        trash_file: file_name,
        project_name: resolved_project.clone(),
    });

    trash_children(&session_id, &resolved_provider, &resolved_project, now_ts, &trash_dir, &mut entries, &state);

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
    let entry = entries
        .iter()
        .find(|e| e.id == trash_id)
        .ok_or_else(|| format!("trash entry not found: {trash_id}"))?
        .clone();

    let remaining: Vec<TrashMeta> = entries.into_iter().filter(|e| e.id != trash_id).collect();

    if entry.trash_file.is_empty() {
        // SOFT-DELETE (shared file): file was never moved.
        atomic_write_json(&meta_path, &remaining)?;
        remove_shared_deletion(&shared_deletions_path, &entry.id, &entry.original_path)?;
        drop(lock);
        sync_source(&entry.provider, &entry.original_path, &state)?;
        return Ok(());
    }

    // DEDICATED FILE: move file back
    let src = trash_dir.join(&entry.trash_file);
    let dest = std::path::Path::new(&entry.original_path);

    if !src.exists() {
        // Already restored or deleted externally
        atomic_write_json(&meta_path, &remaining)?;
        drop(lock);
        sync_source(&entry.provider, &entry.original_path, &state)?;
        return Ok(());
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create parent directory: {e}"))?;
    }

    // Check if other trash entries reference the same file
    let others_use_same_file = remaining.iter().any(|e| e.trash_file == entry.trash_file);

    if others_use_same_file {
        // Copy back, keep trash file for other entries
        if !dest.exists() {
            std::fs::copy(&src, dest).map_err(|e| format!("failed to copy file back: {e}"))?;
        }
    } else {
        // Last reference: move back
        if dest.exists() {
            let _ = std::fs::remove_file(&src);
        } else {
            std::fs::rename(&src, dest)
                .or_else(|_| std::fs::copy(&src, dest).and_then(|_| std::fs::remove_file(&src)))
                .map_err(|e| format!("failed to restore file: {e}"))?;
        }
    }

    atomic_write_json(&meta_path, &remaining)?;
    drop(lock);
    sync_source(&entry.provider, &entry.original_path, &state)?;

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

            // Also permanently delete subagent directory from original location
            if !entry.original_path.is_empty() {
                let original = std::path::Path::new(&entry.original_path);
                let session_dir = original.with_extension("");
                let subagents_dir = session_dir.join("subagents");
                if subagents_dir.is_dir() {
                    let _ = std::fs::remove_dir_all(&subagents_dir);
                    let _ = std::fs::remove_dir(&session_dir);
                }
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

        // Also permanently delete subagent directory from original location
        if !entry.original_path.is_empty() {
            let original = std::path::Path::new(&entry.original_path);
            let session_dir = original.with_extension("");
            let subagents_dir = session_dir.join("subagents");
            if subagents_dir.is_dir() {
                let _ = std::fs::remove_dir_all(&subagents_dir);
                let _ = std::fs::remove_dir(&session_dir);
            }
        }
    }

    let remaining: Vec<TrashMeta> = entries.into_iter().filter(|e| e.id != trash_id).collect();
    atomic_write_json(&meta_path, &remaining)?;

    Ok(())
}

/// Move child session files (subagents) to the trash directory.
/// Works for all providers: Claude subagents in `subagents/` dir, Codex subagents as separate JSONL files.
fn trash_children(
    parent_id: &str,
    provider: &str,
    project_name: &str,
    now_ts: i64,
    trash_dir: &std::path::Path,
    entries: &mut Vec<TrashMeta>,
    state: &AppState,
) {
    let children = match state.db.get_child_sessions(parent_id) {
        Ok(c) => c,
        Err(_) => return,
    };
    for child in &children {
        let child_id = &child.id;
        let child_path = &child.source_path;
        let child_src = std::path::Path::new(child_path);
        if !child_src.exists() || child_path.is_empty() {
            continue;
        }
        let child_base = child_src.file_name().map_or_else(
            || format!("{child_id}.jsonl"),
            |f| f.to_string_lossy().to_string(),
        );
        let child_base = child_base.replace(['/', '\\'], "_");
        let child_name = if let Some(dot_pos) = child_base.rfind('.') {
            format!(
                "{}_{}{}",
                &child_base[..dot_pos],
                now_ts,
                &child_base[dot_pos..]
            )
        } else {
            format!("{child_base}_{now_ts}")
        };
        let child_dest = trash_dir.join(&child_name);
        let _ = std::fs::rename(child_src, &child_dest).or_else(|_| {
            std::fs::copy(child_src, &child_dest).and_then(|_| std::fs::remove_file(child_src))
        });
        entries.push(TrashMeta {
            id: child_id.clone(),
            provider: provider.to_string(),
            title: child.title.clone(),
            original_path: child_path.clone(),
            trashed_at: now_ts,
            trash_file: child_name,
            project_name: project_name.to_string(),
        });
    }
}

fn sync_source(provider_str: &str, source_path: &str, state: &AppState) -> Result<(), String> {
    let provider = crate::models::Provider::parse(provider_str)
        .ok_or_else(|| format!("unsupported provider: {provider_str}"))?;
    sync_source_for_provider(provider, source_path, &state.db)
}
