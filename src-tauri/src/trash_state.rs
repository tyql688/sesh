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

/// Return the set of session IDs that have been deleted from shared sources.
/// Used by the indexer to prevent trashed shared-source sessions from
/// resurrecting during reindex.
pub fn shared_deleted_ids() -> std::collections::HashSet<String> {
    let Ok(dir) = trash_dir() else {
        return std::collections::HashSet::new();
    };
    let path = shared_deletions_path(&dir);
    read_shared_deletions(&path)
        .into_iter()
        .map(|d| d.id)
        .collect()
}

fn read_json_or_default<T>(path: &Path) -> T
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return T::default();
    }

    let data = match std::fs::read_to_string(path) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("failed to read {}: {e}", path.display());
            return T::default();
        }
    };

    match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("failed to parse {}: {e}", path.display());
            let bak = path.with_extension("json.corrupt.bak");
            if let Err(be) = std::fs::copy(path, &bak) {
                log::warn!("failed to backup corrupt file to {}: {be}", bak.display());
            }
            T::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_read_json_or_default_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result: Vec<TrashMeta> = read_json_or_default(&path);
        assert!(result.is_empty());
        assert!(!dir.path().join("nonexistent.json.corrupt.bak").exists());
    }

    #[test]
    fn test_read_json_or_default_corrupt_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trash_meta.json");
        fs::write(&path, "not valid json {{{").unwrap();

        let result: Vec<TrashMeta> = read_json_or_default(&path);
        assert!(result.is_empty());
        let bak = dir.path().join("trash_meta.json.corrupt.bak");
        assert!(bak.exists(), "corrupt backup should be created");
        assert_eq!(fs::read_to_string(&bak).unwrap(), "not valid json {{{");
    }

    #[test]
    fn test_read_json_or_default_valid_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("shared_deletions.json");
        let data = vec![SharedDeletion {
            id: "s1".into(),
            provider: "opencode".into(),
            original_path: "/db".into(),
        }];
        fs::write(&path, serde_json::to_string(&data).unwrap()).unwrap();

        let result: Vec<SharedDeletion> = read_json_or_default(&path);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "s1");
        assert!(!path.with_extension("json.corrupt.bak").exists());
    }
}
