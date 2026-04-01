use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::models::{Message, Provider, SessionMeta};

/// Result of trashing a session.
pub enum TrashResult {
    /// File was moved to the trash directory.
    Moved { trash_file: String },
    /// Shared source — session soft-deleted (no file moved).
    SoftDeleted,
}

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Clone)]
pub struct ParsedSession {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
    pub content_text: String,
}

/// Static metadata for a provider. Implemented by zero-sized descriptor structs
/// in each provider module. Accessed via `Provider::descriptor()`.
pub trait ProviderDescriptor: Send + Sync {
    /// Whether source files contain multiple sessions.
    fn is_shared_source(&self) -> bool {
        false
    }

    /// Check if a source file path belongs to this provider.
    fn owns_source_path(&self, source_path: &str) -> bool;

    /// Build the CLI resume command for a session.
    fn resume_command(&self, session_id: &str, variant_name: Option<&str>) -> Option<String>;

    /// Key used to group sessions in the tree.
    fn display_key(&self, variant_name: Option<&str>) -> String;

    /// Try to parse a display key as belonging to this provider.
    /// Returns the display label if the key matches a custom format.
    /// Default: None (handled by Provider::parse fallback).
    fn try_parse_display_key(&self, _display_key: &str) -> Option<String> {
        None
    }

    /// Sort order for provider groups in the tree.
    fn sort_order(&self) -> u32;

    /// Provider brand color (hex).
    fn color(&self) -> &'static str;

    /// CLI command name for the security whitelist (e.g. "claude", "agent").
    /// Empty string if dynamic (e.g. cc-mirror variants).
    fn cli_command(&self) -> &'static str;

    /// SVG icon for HTML export. Returns a complete `<svg>` element or empty string.
    fn avatar_svg(&self) -> &'static str {
        ""
    }
}

impl Provider {
    /// Get the descriptor for this provider (static metadata).
    pub fn descriptor(&self) -> &'static dyn ProviderDescriptor {
        match self {
            Provider::Claude => &crate::providers::claude::Descriptor,
            Provider::Codex => &crate::providers::codex::Descriptor,
            Provider::Gemini => &crate::providers::gemini::Descriptor,
            Provider::Cursor => &crate::providers::cursor::Descriptor,
            Provider::OpenCode => &crate::providers::opencode::Descriptor,
            Provider::Kimi => &crate::providers::kimi::Descriptor,
            Provider::CcMirror => &crate::providers::cc_mirror::Descriptor,
        }
    }

    /// Identify which provider owns a source path.
    pub fn from_source_path(source_path: &str) -> Option<Provider> {
        Provider::all()
            .iter()
            .find(|p| p.descriptor().owns_source_path(source_path))
            .cloned()
    }

    /// Parse a display key (as produced by `descriptor().display_key()`) back to a provider and label.
    /// Handles cc-mirror variants like "cc-mirror:cczai" → (CcMirror, "cczai").
    pub fn parse_display_key(display_key: &str) -> Option<(Provider, String)> {
        // Direct match: covers most providers
        if let Some(p) = Provider::parse(display_key) {
            let label = p.label().to_string();
            return Some((p, label));
        }
        // Custom formats: e.g. "cc-mirror:variant"
        for p in Provider::all() {
            if let Some(label) = p.descriptor().try_parse_display_key(display_key) {
                return Some((p.clone(), label));
            }
        }
        None
    }
}

/// Move a source file to the trash directory. Shared helper for `trash_session` implementations.
pub fn move_to_trash(
    source_path: &Path,
    trash_dir: &Path,
    timestamp: i64,
) -> Result<TrashResult, ProviderError> {
    let base_name = source_path.file_name().map_or_else(
        || "session".to_string(),
        |f| f.to_string_lossy().to_string(),
    );
    let base_name = base_name.replace(['/', '\\'], "_");
    let file_name = if let Some(dot_pos) = base_name.rfind('.') {
        format!(
            "{}_{}{}",
            &base_name[..dot_pos],
            timestamp,
            &base_name[dot_pos..]
        )
    } else {
        format!("{base_name}_{timestamp}")
    };
    let dest = trash_dir.join(&file_name);
    std::fs::rename(source_path, &dest)
        .or_else(|_| {
            std::fs::copy(source_path, &dest).and_then(|_| std::fs::remove_file(source_path))
        })
        .map_err(ProviderError::Io)?;
    Ok(TrashResult::Moved {
        trash_file: file_name,
    })
}

/// Create a provider instance by enum variant. Returns None if HOME is unavailable.
pub fn make_provider(provider: &Provider) -> Option<Box<dyn SessionProvider>> {
    match provider {
        Provider::Claude => crate::providers::claude::ClaudeProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Codex => crate::providers::codex::CodexProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Gemini => crate::providers::gemini::GeminiProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Cursor => crate::providers::cursor::CursorProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::OpenCode => crate::providers::opencode::OpenCodeProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Kimi => crate::providers::kimi::KimiProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::CcMirror => crate::providers::cc_mirror::CcMirrorProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
    }
}

/// Create all provider instances, silently skipping any that cannot resolve HOME.
pub fn all_providers() -> Vec<Box<dyn SessionProvider>> {
    Provider::all().iter().filter_map(make_provider).collect()
}

pub trait SessionProvider: Send + Sync {
    fn provider(&self) -> Provider;
    fn watch_paths(&self) -> Vec<PathBuf>;
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError>;
    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        Ok(self
            .scan_all()?
            .into_iter()
            .filter(|session| session.meta.source_path == source_path)
            .collect())
    }
    fn load_messages(
        &self,
        session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError>;

    /// Delete a session's data from its shared source file.
    /// Only called for providers where `descriptor().is_shared_source()` returns true.
    fn delete_from_source(
        &self,
        _source_path: &str,
        _session_id: &str,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    /// Trash a session's source file. Default: move file to trash directory.
    /// Shared-source providers should override to return `SoftDeleted`.
    fn trash_session(
        &self,
        source_path: &Path,
        trash_dir: &Path,
        timestamp: i64,
    ) -> Result<TrashResult, ProviderError> {
        move_to_trash(source_path, trash_dir, timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_source_path() {
        let cases = [
            (
                "/home/user/.claude/projects/foo/abc.jsonl",
                Some(Provider::Claude),
            ),
            (
                "/home/user/.codex/sessions/xyz.jsonl",
                Some(Provider::Codex),
            ),
            (
                "/home/user/.gemini/tmp/proj/chats/session.json",
                Some(Provider::Gemini),
            ),
            (
                "/home/user/.cursor/chats/uuid/store.db",
                Some(Provider::Cursor),
            ),
            (
                "/home/user/.local/share/opencode/opencode.db",
                Some(Provider::OpenCode),
            ),
            (
                "/home/user/.kimi/sessions/hash/uuid/wire.jsonl",
                Some(Provider::Kimi),
            ),
            (
                "/home/user/.cc-mirror/variant/config/projects/foo/abc.jsonl",
                Some(Provider::CcMirror),
            ),
            ("/home/user/random/file.txt", None),
            // cc-mirror path should NOT match claude
            (
                "/home/user/.cc-mirror/cczai/config/projects/foo/abc.jsonl",
                Some(Provider::CcMirror),
            ),
        ];
        for (path, expected) in &cases {
            assert_eq!(
                Provider::from_source_path(path).as_ref(),
                expected.as_ref(),
                "from_source_path({path})"
            );
        }
    }

    #[test]
    fn test_parse_display_key() {
        // Regular providers
        assert_eq!(
            Provider::parse_display_key("claude"),
            Some((Provider::Claude, "Claude Code".to_string()))
        );
        assert_eq!(
            Provider::parse_display_key("codex"),
            Some((Provider::Codex, "Codex".to_string()))
        );
        // CC-Mirror variants
        assert_eq!(
            Provider::parse_display_key("cc-mirror:cczai"),
            Some((Provider::CcMirror, "cczai".to_string()))
        );
        // Unknown
        assert_eq!(Provider::parse_display_key("unknown"), None);
    }

    #[test]
    fn test_display_key_roundtrip() {
        // Regular providers roundtrip through parse_display_key
        for p in Provider::all() {
            if *p == Provider::CcMirror {
                continue; // cc-mirror needs variant_name
            }
            let key = p.descriptor().display_key(None);
            let parsed = Provider::parse_display_key(&key);
            assert!(parsed.is_some(), "display_key roundtrip failed for {:?}", p);
            assert_eq!(parsed.unwrap().0, *p);
        }
        // CC-Mirror with variant
        let key = Provider::CcMirror.descriptor().display_key(Some("cczai"));
        let parsed = Provider::parse_display_key(&key);
        assert_eq!(parsed, Some((Provider::CcMirror, "cczai".to_string())));
    }

    #[test]
    fn test_descriptor_sort_order_unique() {
        let mut orders: Vec<u32> = Provider::all()
            .iter()
            .map(|p| p.descriptor().sort_order())
            .collect();
        orders.sort();
        orders.dedup();
        assert_eq!(
            orders.len(),
            Provider::all().len(),
            "sort_order values must be unique"
        );
    }
}
