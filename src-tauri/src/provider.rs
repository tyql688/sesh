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
