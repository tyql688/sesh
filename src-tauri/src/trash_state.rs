use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::TrashMeta;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SharedDeletion {
    pub id: String,
    pub provider: String,
    pub original_path: String,
}

pub fn trash_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_local_dir()
        .ok_or_else(|| "failed to resolve local data dir".to_string())?
        .join("cc-session")
        .join("trash");
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create trash directory: {e}"))?;
    Ok(dir)
}

pub fn trash_meta_path(trash_dir: &Path) -> PathBuf {
    trash_dir.join("trash_meta.json")
}

pub fn shared_deletions_path(trash_dir: &Path) -> PathBuf {
    trash_dir.join("shared_deletions.json")
}

pub fn atomic_write_json<T: Serialize>(path: &Path, data: &T) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("failed to serialize trash state: {e}"))?;
    std::fs::write(&tmp, &json).map_err(|e| format!("failed to write trash state tmp: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("failed to rename trash state: {e}"))?;
    Ok(())
}

pub fn read_trash_meta(meta_path: &Path) -> Vec<TrashMeta> {
    read_json_or_default(meta_path)
}

pub fn read_shared_deletions(path: &Path) -> Vec<SharedDeletion> {
    read_json_or_default(path)
}

pub fn remove_shared_deletion(
    path: &Path,
    session_id: &str,
    original_path: &str,
) -> Result<(), String> {
    let filtered: Vec<SharedDeletion> = read_shared_deletions(path)
        .into_iter()
        .filter(|entry| !(entry.id == session_id && entry.original_path == original_path))
        .collect();
    atomic_write_json(path, &filtered)
}

pub fn add_shared_deletion(
    path: &Path,
    session_id: &str,
    provider: &str,
    original_path: &str,
) -> Result<(), String> {
    let mut entries = read_shared_deletions(path);
    let candidate = SharedDeletion {
        id: session_id.to_string(),
        provider: provider.to_string(),
        original_path: original_path.to_string(),
    };

    if !entries.iter().any(|entry| entry == &candidate) {
        entries.push(candidate);
        atomic_write_json(path, &entries)?;
    }

    Ok(())
}

pub fn active_shared_deletions_by_source() -> HashMap<String, HashSet<String>> {
    let Ok(trash_dir) = trash_dir() else {
        return HashMap::new();
    };

    let mut by_source: HashMap<String, HashSet<String>> = HashMap::new();

    for entry in read_trash_meta(&trash_meta_path(&trash_dir))
        .into_iter()
        .filter(|entry| entry.trash_file.is_empty() && !entry.original_path.is_empty())
    {
        by_source
            .entry(entry.original_path)
            .or_default()
            .insert(entry.id);
    }

    for entry in read_shared_deletions(&shared_deletions_path(&trash_dir)) {
        by_source
            .entry(entry.original_path)
            .or_default()
            .insert(entry.id);
    }

    by_source
}

fn read_json_or_default<T>(path: &Path) -> T
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return T::default();
    }

    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}
