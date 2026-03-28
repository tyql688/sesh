use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::provider::ParsedSession;

pub fn chat_session_ids(project_dir: Option<&std::path::PathBuf>) -> HashSet<String> {
    let mut ids = HashSet::new();
    let Some(project_dir) = project_dir else {
        return ids;
    };

    let chats_dir = project_dir.join("chats");
    let Ok(entries) = fs::read_dir(chats_dir) else {
        return ids;
    };

    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        let Some(file_name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            continue;
        };

        if file_name.starts_with("session-") && file_name.ends_with(".json") {
            ids.insert(
                file_name
                    .trim_start_matches("session-")
                    .trim_end_matches(".json")
                    .to_string(),
            );
        }
    }

    ids
}

/// Collect "real" session ID prefixes from the chats/ directory.
/// Gemini CLI stores real sessions as `chats/session-DATE-IDPREFIX.json`.
/// Sessions NOT in this list are orphans (e.g. image sends with new sessionIds).
pub fn collect_real_session_prefixes(project_dir: &Path) -> Vec<String> {
    let chats_dir = project_dir.join("chats");
    let mut prefixes = Vec::new();
    if let Ok(entries) = fs::read_dir(&chats_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Format: session-2026-03-27T07-02-63f10611.json
            if let Some(stem) = name.strip_suffix(".json") {
                if let Some(last_dash) = stem.rfind('-') {
                    prefixes.push(stem[last_dash + 1..].to_string());
                }
            }
        }
    }
    // Fallback: check for UUID-named directories (older Gemini versions)
    if prefixes.is_empty() {
        if let Ok(entries) = fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.path().is_dir() && name.len() > 30 && name.contains('-') {
                    prefixes.push(name[..8].to_string());
                }
            }
        }
    }
    prefixes
}

/// Merge orphan sessions (not in chats/) into the nearest real session.
pub fn merge_orphan_sessions(sessions: &mut Vec<ParsedSession>, real_prefixes: &[String]) {
    if sessions.len() < 2 || real_prefixes.is_empty() {
        return;
    }

    sessions.sort_by_key(|s| s.meta.created_at);

    let is_real = |sid: &str| -> bool {
        real_prefixes.iter().any(|p| sid.starts_with(p))
    };

    // Identify orphan vs real indices
    let orphan_indices: Vec<usize> = sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| !is_real(&s.meta.id))
        .map(|(i, _)| i)
        .collect();

    if orphan_indices.is_empty() {
        return;
    }

    // For each orphan, find nearest real session (prefer preceding)
    let mut merges: Vec<(usize, usize)> = Vec::new();
    for &orphan_idx in &orphan_indices {
        let mut target: Option<usize> = None;
        for j in (0..orphan_idx).rev() {
            if !orphan_indices.contains(&j) {
                target = Some(j);
                break;
            }
        }
        if target.is_none() {
            for j in (orphan_idx + 1)..sessions.len() {
                if !orphan_indices.contains(&j) {
                    target = Some(j);
                    break;
                }
            }
        }
        if let Some(t) = target {
            merges.push((orphan_idx, t));
        }
    }

    // Apply merges: first merge data, then remove orphans
    for &(orphan_idx, target_idx) in &merges {
        if orphan_idx >= sessions.len() || target_idx >= sessions.len() {
            continue;
        }
        let orphan = sessions[orphan_idx].clone();
        let target = &mut sessions[target_idx];
        target.messages.extend(orphan.messages);
        target.meta.message_count = target.messages.len() as u32;
        if orphan.meta.updated_at > target.meta.updated_at {
            target.meta.updated_at = orphan.meta.updated_at;
        }
        if !orphan.content_text.is_empty() {
            target.content_text.push('\n');
            target.content_text.push_str(&orphan.content_text);
        }
    }

    // Remove orphans in reverse index order to preserve indices
    let mut remove_indices: Vec<usize> = merges.iter().map(|(o, _)| *o).collect();
    remove_indices.sort_unstable_by(|a, b| b.cmp(a));
    remove_indices.dedup();
    for idx in remove_indices {
        if idx < sessions.len() {
            sessions.remove(idx);
        }
    }
}
