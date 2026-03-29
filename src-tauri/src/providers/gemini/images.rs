use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::provider_utils::NO_PROJECT;

/// Strip `@path/to/image.png` references from text, keeping only non-path caption text.
/// Used when inlineData already provides the image.
pub fn strip_at_image_refs(text: &str) -> String {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('@') {
                // "@path/to/image.png caption" -> keep caption only
                let after_at = trimmed.strip_prefix('@').unwrap_or(trimmed).trim();
                if let Some(space_idx) = after_at.find(|c: char| c.is_whitespace()) {
                    let path_part = &after_at[..space_idx];
                    if looks_like_image_path(path_part) {
                        let rest = after_at[space_idx..].trim();
                        return if rest.is_empty() {
                            None
                        } else {
                            Some(rest.to_string())
                        };
                    }
                }
                // Entire line is just @path
                if looks_like_image_path(after_at) {
                    return None;
                }
            }
            Some(line.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn resolve_gemini_image_path(raw: &str, project_path: &str) -> Option<String> {
    let candidate = raw.strip_prefix('@').unwrap_or(raw).trim();
    if !looks_like_image_path(candidate) {
        return None;
    }

    let candidate_path = PathBuf::from(candidate);
    let resolved = if candidate_path.is_absolute() {
        candidate_path
    } else if project_path != NO_PROJECT {
        normalize_path(&PathBuf::from(project_path).join(&candidate_path))
    } else if let Some(home_dir) = dirs::home_dir() {
        if let Some(index) = candidate
            .find(".gemini/")
            .or_else(|| candidate.find(".gemini\\"))
        {
            normalize_path(&home_dir.join(&candidate[index..]))
        } else {
            normalize_path(&home_dir.join(&candidate_path))
        }
    } else {
        return None;
    };

    let final_path = fs::canonicalize(&resolved).unwrap_or(resolved);

    // Guard against path traversal: resolved path must be within allowed directories
    let path_str = final_path.to_string_lossy();
    if let Some(home) = dirs::home_dir() {
        let home_str = home.to_string_lossy();
        #[cfg(not(target_os = "windows"))]
        let allowed = path_str.starts_with(&*home_str)
            || path_str.starts_with("/tmp/")
            || path_str.starts_with("/private/tmp/")
            || path_str.starts_with("/var/folders/");
        #[cfg(target_os = "windows")]
        let allowed = {
            let mut a = path_str.starts_with(&*home_str);
            if let Ok(temp) = std::env::var("TEMP") {
                a = a || path_str.starts_with(&temp);
            }
            if let Ok(tmp) = std::env::var("TMP") {
                a = a || path_str.starts_with(&tmp);
            }
            a
        };
        if !allowed {
            return None;
        }
    }

    Some(path_str.to_string())
}

pub fn looks_like_image_path(candidate: &str) -> bool {
    let lower = candidate.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }

    normalized
}
