use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::models::{Message, Provider, SessionMeta, TrashMeta};

// ---------------------------------------------------------------------------
// Deletion plan types — provider returns a plan, command layer executes it
// ---------------------------------------------------------------------------

/// What to do with a session's source file during deletion.
#[derive(Debug, Clone, PartialEq)]
pub enum FileAction {
    /// Move/delete the source file (dedicated file per session).
    Remove,
    /// Shared source — don't touch the file.
    /// On permanent delete, call `purge_from_source()`.
    Shared,
    /// Don't touch the file and no purge needed
    /// (e.g. child session embedded in parent's file).
    Skip,
}

/// How to restore a trashed session.
#[derive(Debug, Clone, PartialEq)]
pub enum RestoreAction {
    /// Move the trash file back to original_path.
    MoveBack,
    /// Remove from shared_deletions tracking, then re-sync source.
    UndoSharedDeletion,
    /// Nothing to restore (embedded child — parent restore handles it).
    Noop,
}

/// Plan for deleting a child session.
#[derive(Debug, Clone)]
pub struct ChildPlan {
    pub id: String,
    pub source_path: String,
    pub title: String,
    pub file_action: FileAction,
}

/// Complete deletion plan returned by provider.
/// Command layer executes this mechanically — zero provider logic.
#[derive(Debug, Clone)]
pub struct DeletionPlan {
    pub file_action: FileAction,
    pub child_plans: Vec<ChildPlan>,
    /// Extra directories to remove after file operations.
    pub cleanup_dirs: Vec<PathBuf>,
}

// ---------------------------------------------------------------------------
// Deletion plan execution — shared by trash_session, delete_session, batch
// ---------------------------------------------------------------------------

/// Execute a trash operation: move files to trash dir, return metadata records.
pub fn execute_trash(
    plan: &DeletionPlan,
    meta: &SessionMeta,
    provider_key: &str,
    trash_dir: &Path,
    ts: i64,
) -> Result<Vec<TrashMeta>, String> {
    let mut records = Vec::new();

    // Main session
    let trash_file = match plan.file_action {
        FileAction::Remove => {
            let src = Path::new(&meta.source_path);
            if src.exists() {
                match move_to_trash(src, trash_dir, ts) {
                    Ok(TrashResult::Moved { trash_file }) => trash_file,
                    Err(e) => return Err(format!("failed to move parent to trash: {e}")),
                }
            } else {
                String::new()
            }
        }
        FileAction::Shared | FileAction::Skip => String::new(),
    };
    records.push(TrashMeta {
        id: meta.id.clone(),
        provider: provider_key.to_string(),
        title: meta.title.clone(),
        original_path: meta.source_path.clone(),
        trashed_at: ts,
        trash_file,
        project_name: meta.project_name.clone(),
        parent_id: None,
    });

    // Children
    for child in &plan.child_plans {
        let child_trash_file = match child.file_action {
            FileAction::Remove => {
                let src = Path::new(&child.source_path);
                if src.exists() {
                    match move_to_trash(src, trash_dir, ts) {
                        Ok(TrashResult::Moved { trash_file }) => trash_file,
                        Err(e) => {
                            return Err(format!("failed to move child {} to trash: {e}", child.id))
                        }
                    }
                } else {
                    String::new()
                }
            }
            FileAction::Shared | FileAction::Skip => String::new(),
        };
        records.push(TrashMeta {
            id: child.id.clone(),
            provider: provider_key.to_string(),
            title: child.title.clone(),
            original_path: child.source_path.clone(),
            trashed_at: ts,
            trash_file: child_trash_file,
            project_name: meta.project_name.clone(),
            parent_id: Some(meta.id.clone()),
        });
    }

    // Cleanup directories
    for dir in &plan.cleanup_dirs {
        if dir.is_dir() {
            std::fs::remove_dir_all(dir).map_err(|e| {
                format!("failed to remove cleanup directory {}: {e}", dir.display())
            })?;
        }
    }

    Ok(records)
}

/// Execute a permanent delete: remove files or purge from shared source.
pub fn execute_purge(
    plan: &DeletionPlan,
    provider: &dyn SessionProvider,
    meta: &SessionMeta,
) -> Result<(), String> {
    match plan.file_action {
        FileAction::Remove => {
            let src = Path::new(&meta.source_path);
            if src.exists() {
                std::fs::remove_file(src).map_err(|e| {
                    format!("failed to remove parent source file {}: {e}", src.display())
                })?;
            }
        }
        FileAction::Shared => {
            provider
                .purge_from_source(&meta.source_path, &meta.id)
                .map_err(|e| format!("failed to purge parent from shared source: {e}"))?;
        }
        FileAction::Skip => {}
    }

    for child in &plan.child_plans {
        match child.file_action {
            FileAction::Remove => {
                let src = Path::new(&child.source_path);
                if src.exists() {
                    std::fs::remove_file(src).map_err(|e| {
                        format!("failed to remove child source file {}: {e}", src.display())
                    })?;
                }
                // Also try .meta.json (Claude subagents)
                let meta_path = src.with_extension("meta.json");
                if meta_path.exists() {
                    std::fs::remove_file(&meta_path).map_err(|e| {
                        format!(
                            "failed to remove child metadata file {}: {e}",
                            meta_path.display()
                        )
                    })?;
                }
            }
            FileAction::Shared => {
                provider
                    .purge_from_source(&child.source_path, &child.id)
                    .map_err(|e| {
                        format!("failed to purge child {} from shared source: {e}", child.id)
                    })?;
            }
            FileAction::Skip => {}
        }
    }

    for dir in &plan.cleanup_dirs {
        if dir.is_dir() {
            std::fs::remove_dir_all(dir).map_err(|e| {
                format!("failed to remove cleanup directory {}: {e}", dir.display())
            })?;
        }
    }
    Ok(())
}

/// Execute a restore: move file back or undo shared deletion.
/// Returns `true` if a source sync is needed after restore.
pub fn execute_restore(
    action: &RestoreAction,
    entry: &TrashMeta,
    trash_dir: &Path,
    all_entries: &[TrashMeta],
) -> Result<bool, String> {
    match action {
        RestoreAction::MoveBack => {
            if entry.trash_file.is_empty() {
                return Ok(false);
            }
            let src = trash_dir.join(&entry.trash_file);
            let dest = Path::new(&entry.original_path);

            if !src.exists() {
                // Already restored or deleted externally
                return Ok(true);
            }

            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create parent directory: {e}"))?;
            }

            // Check if other trash entries reference the same trash file
            let others_use_same_file = all_entries
                .iter()
                .any(|e| e.id != entry.id && e.trash_file == entry.trash_file);

            if others_use_same_file {
                if !dest.exists() {
                    std::fs::copy(&src, dest)
                        .map_err(|e| format!("failed to copy file back: {e}"))?;
                }
            } else if dest.exists() {
                let _ = std::fs::remove_file(&src);
            } else {
                std::fs::rename(&src, dest)
                    .or_else(|_| std::fs::copy(&src, dest).and_then(|_| std::fs::remove_file(&src)))
                    .map_err(|e| format!("failed to restore file: {e}"))?;
            }

            Ok(true)
        }
        RestoreAction::UndoSharedDeletion => {
            // Caller handles remove_shared_deletion + sync_source
            Ok(true)
        }
        RestoreAction::Noop => Ok(false),
    }
}

/// Shared deletion plan for JSONL providers with subagent directories
/// (Claude, CC-Mirror, and similar).
/// - Parent: Remove file + Remove children + cleanup session dir
/// - Child: Remove own file only
pub fn jsonl_subagents_deletion_plan(meta: &SessionMeta, children: &[SessionMeta]) -> DeletionPlan {
    if meta.parent_id.is_some() {
        return DeletionPlan {
            file_action: FileAction::Remove,
            child_plans: Vec::new(),
            cleanup_dirs: Vec::new(),
        };
    }

    let child_plans = children
        .iter()
        .map(|c| ChildPlan {
            id: c.id.clone(),
            source_path: c.source_path.clone(),
            title: c.title.clone(),
            file_action: FileAction::Remove,
        })
        .collect();

    // Session dir: /path/to/{session_id}/ (may contain subagents/, context.jsonl, state.json)
    let source = PathBuf::from(&meta.source_path);
    let session_dir = source.with_extension("");
    let mut cleanup_dirs = Vec::new();
    if session_dir.is_dir() {
        cleanup_dirs.push(session_dir);
    }

    DeletionPlan {
        file_action: FileAction::Remove,
        child_plans,
        cleanup_dirs,
    }
}

/// Result of trashing a session (internal, used by move_to_trash).
enum TrashResult {
    Moved { trash_file: String },
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

/// Generate a trash-safe filename by sanitizing and inserting a timestamp.
fn trash_file_name(source_path: &Path, timestamp: i64) -> String {
    let base_name = source_path.file_name().map_or_else(
        || "session".to_string(),
        |f| f.to_string_lossy().to_string(),
    );
    let base_name = base_name.replace(['/', '\\'], "_");
    match base_name.rfind('.') {
        Some(dot_pos) => {
            let (name, ext) = base_name.split_at(dot_pos);
            format!("{name}_{timestamp}{ext}")
        }
        None => format!("{base_name}_{timestamp}"),
    }
}

/// Move a source file to the trash directory. Shared helper for `trash_session` implementations.
fn move_to_trash(
    source_path: &Path,
    trash_dir: &Path,
    timestamp: i64,
) -> Result<TrashResult, ProviderError> {
    let file_name = trash_file_name(source_path, timestamp);
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

    /// Return a deletion plan for this session.
    /// Provider decides all file actions; command layer executes mechanically.
    fn deletion_plan(&self, meta: &SessionMeta, children: &[SessionMeta]) -> DeletionPlan;

    /// Determine how to restore a trashed session.
    /// Default: MoveBack for dedicated files, UndoSharedDeletion for shared.
    /// If neither applies (e.g. failed move before metadata write), do no-op.
    fn restore_action(&self, entry: &TrashMeta) -> RestoreAction {
        infer_restore_action(entry)
    }

    /// Permanently remove session data from a shared source (DB/file).
    /// Called by `execute_purge` when `FileAction::Shared`.
    /// Default: no-op (dedicated-file providers don't need this).
    fn purge_from_source(
        &self,
        _source_path: &str,
        _session_id: &str,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    /// Additional cleanup when a session is permanently deleted (empty trash / permanent delete).
    /// Called after the main file and directory cleanup.
    /// Default: no-op. Override to clean up provider-specific external data.
    fn cleanup_on_permanent_delete(&self, _session_id: &str) {}
}

fn is_shared_source_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    normalized.ends_with(".db") || normalized.ends_with("/logs.json")
}

pub fn infer_restore_action(entry: &TrashMeta) -> RestoreAction {
    if !entry.trash_file.is_empty() {
        RestoreAction::MoveBack
    } else if is_shared_source_path(&entry.original_path) {
        RestoreAction::UndoSharedDeletion
    } else {
        RestoreAction::Noop
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct DummyProvider;

    impl SessionProvider for DummyProvider {
        fn provider(&self) -> Provider {
            Provider::Claude
        }
        fn watch_paths(&self) -> Vec<PathBuf> {
            Vec::new()
        }
        fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
            Ok(Vec::new())
        }
        fn load_messages(
            &self,
            _session_id: &str,
            _source_path: &str,
        ) -> Result<Vec<Message>, ProviderError> {
            Ok(Vec::new())
        }
        fn deletion_plan(&self, _meta: &SessionMeta, _children: &[SessionMeta]) -> DeletionPlan {
            DeletionPlan {
                file_action: FileAction::Skip,
                child_plans: Vec::new(),
                cleanup_dirs: Vec::new(),
            }
        }
        fn purge_from_source(
            &self,
            _source_path: &str,
            _session_id: &str,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Parse("boom".to_string()))
        }
    }

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
                "/home/user/.cursor/projects/slug/agent-transcripts/parent-id/parent-id.jsonl",
                Some(Provider::Cursor),
            ),
            (
                "/home/user/.cursor/projects/slug/agent-transcripts/parent-id/subagents/child-id.jsonl",
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

    #[test]
    fn test_default_restore_action_noop_for_empty_trash_file_on_dedicated_source() {
        let provider = DummyProvider;
        let entry = TrashMeta {
            id: "s1".to_string(),
            provider: "claude".to_string(),
            title: "t".to_string(),
            original_path: "/tmp/session.jsonl".to_string(),
            trashed_at: 0,
            trash_file: String::new(),
            project_name: String::new(),
            parent_id: None,
        };
        assert_eq!(provider.restore_action(&entry), RestoreAction::Noop);
    }

    #[test]
    fn test_default_restore_action_shared_for_db_source() {
        let provider = DummyProvider;
        let entry = TrashMeta {
            id: "s1".to_string(),
            provider: "cursor".to_string(),
            title: "t".to_string(),
            original_path: "/tmp/store.db".to_string(),
            trashed_at: 0,
            trash_file: String::new(),
            project_name: String::new(),
            parent_id: None,
        };
        assert_eq!(
            provider.restore_action(&entry),
            RestoreAction::UndoSharedDeletion
        );
    }

    #[test]
    fn test_execute_purge_propagates_shared_purge_errors() {
        let provider = DummyProvider;
        let plan = DeletionPlan {
            file_action: FileAction::Shared,
            child_plans: Vec::new(),
            cleanup_dirs: Vec::new(),
        };
        let meta = SessionMeta {
            id: "s1".to_string(),
            provider: Provider::Claude,
            title: "t".to_string(),
            project_path: String::new(),
            project_name: String::new(),
            created_at: 0,
            updated_at: 0,
            message_count: 0,
            file_size_bytes: 0,
            source_path: "/tmp/store.db".to_string(),
            is_sidechain: false,
            variant_name: None,
            model: None,
            cc_version: None,
            git_branch: None,
            parent_id: None,
        };

        let err = execute_purge(&plan, &provider, &meta).expect_err("should propagate purge error");
        assert!(err.contains("boom"));
    }

    #[test]
    fn test_infer_restore_action_moveback_when_trash_file_exists() {
        let entry = TrashMeta {
            id: "s1".to_string(),
            provider: "legacy-cursor".to_string(),
            title: "t".to_string(),
            original_path: "/tmp/agent-transcripts/s1/s1.jsonl".to_string(),
            trashed_at: 0,
            trash_file: "1710000000__s1.jsonl".to_string(),
            project_name: String::new(),
            parent_id: None,
        };
        assert_eq!(infer_restore_action(&entry), RestoreAction::MoveBack);
    }
}
