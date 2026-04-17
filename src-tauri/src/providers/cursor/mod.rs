// Currently only CLI sessions are processed. IDE (Composer) sessions share the
// same agent-transcripts JSONL format but lack a corresponding store.db under
// ~/.cursor/chats/. We use store.db existence as the hard signal: if a session
// ID has a store.db, it is CLI; otherwise it is IDE and gets skipped entirely.

mod parser;
mod tools;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::models::{Message, Provider, SessionMeta};
use crate::provider::{
    ChildPlan, DeletionPlan, FileAction, LoadedSession, ParsedSession, ProviderError,
    SessionProvider,
};
use crate::provider_utils::project_name_from_path;
use rayon::prelude::*;
use rusqlite::Connection;
use serde_json::Value;

pub struct Descriptor;
impl crate::provider::ProviderDescriptor for Descriptor {
    fn owns_source_path(&self, source_path: &str) -> bool {
        let p = source_path.replace('\\', "/");
        p.contains("/.cursor/projects/") && p.contains("/agent-transcripts/")
    }
    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("agent --resume={session_id}"))
    }
    fn display_key(&self, _variant_name: Option<&str>) -> String {
        "cursor".into()
    }
    fn sort_order(&self) -> u32 {
        4
    }
    fn color(&self) -> &'static str {
        "#3b82f6"
    }
    fn cli_command(&self) -> &'static str {
        "agent"
    }
    fn avatar_svg(&self) -> &'static str {
        r#"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z" fill="currentColor"/></svg>"#
    }
}

pub struct CursorProvider {
    home_dir: PathBuf,
}

impl CursorProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    fn projects_dir(&self) -> PathBuf {
        self.home_dir.join(".cursor").join("projects")
    }

    fn chats_dir(&self) -> PathBuf {
        self.home_dir.join(".cursor").join("chats")
    }

    /// Scan ~/.cursor/chats/<hash>/<sessionId>/store.db to collect all CLI
    /// session IDs and their store.db paths.
    fn collect_cli_session_stores(&self) -> HashMap<String, PathBuf> {
        let chats_dir = self.chats_dir();
        let mut stores = HashMap::new();
        if !chats_dir.exists() {
            return stores;
        }
        let Ok(hash_entries) = std::fs::read_dir(&chats_dir) else {
            return stores;
        };
        for hash_entry in hash_entries.flatten() {
            let hash_dir = hash_entry.path();
            if !hash_dir.is_dir() {
                continue;
            }
            let Ok(session_entries) = std::fs::read_dir(&hash_dir) else {
                continue;
            };
            for session_entry in session_entries.flatten() {
                let session_dir = session_entry.path();
                let store_db = session_dir.join("store.db");
                if session_dir.is_dir() && store_db.exists() {
                    if let Some(name) = session_dir.file_name().and_then(|n| n.to_str()) {
                        stores.insert(name.to_string(), store_db);
                    }
                }
            }
        }
        stores
    }

    /// Collect all transcript JSONL files under agent-transcripts/.
    /// Returns (main_transcripts, subagent_transcripts).
    fn collect_transcript_files(&self) -> (Vec<PathBuf>, Vec<PathBuf>) {
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return (Vec::new(), Vec::new());
        }

        let mut mains = Vec::new();
        let mut subagents = Vec::new();

        let project_entries = match std::fs::read_dir(&projects_dir) {
            Ok(e) => e,
            Err(_) => return (Vec::new(), Vec::new()),
        };

        for project_entry in project_entries.flatten() {
            let transcripts_dir = project_entry.path().join("agent-transcripts");
            if !transcripts_dir.is_dir() {
                continue;
            }
            let Ok(session_entries) = std::fs::read_dir(&transcripts_dir) else {
                continue;
            };
            for session_entry in session_entries.flatten() {
                let session_dir = session_entry.path();
                if !session_dir.is_dir() {
                    continue;
                }
                let dir_name = session_dir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");

                // Main transcript: <sessionId>/<sessionId>.jsonl
                let main_jsonl = session_dir.join(format!("{dir_name}.jsonl"));
                if main_jsonl.is_file() {
                    mains.push(main_jsonl);
                }

                // Subagent transcripts: <sessionId>/subagents/*.jsonl
                let subagents_dir = session_dir.join("subagents");
                if subagents_dir.is_dir() {
                    if let Ok(sa_entries) = std::fs::read_dir(&subagents_dir) {
                        for sa_entry in sa_entries.flatten() {
                            let sa_path = sa_entry.path();
                            if sa_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                                subagents.push(sa_path);
                            }
                        }
                    }
                }
            }
        }

        (mains, subagents)
    }

    /// Load messages from a transcript JSONL file.
    /// Format: `{role, message: {content: [{type: "text"/"tool_use", ...}]}}`.
    fn load_transcript_messages(&self, source_path: &str) -> Result<Vec<Message>, ProviderError> {
        let content = std::fs::read_to_string(source_path)
            .map_err(|e| ProviderError::Parse(format!("failed to read transcript: {e}")))?;
        Ok(crate::providers::cursor::parser::parse_transcript_messages(
            &content,
            source_path,
        ))
    }

    fn extract_workspace_path_from_store_db(&self, store_db_path: &Path) -> Option<String> {
        let conn = match Connection::open(store_db_path) {
            Ok(conn) => conn,
            Err(error) => {
                log::warn!(
                    "failed to open Cursor store DB '{}': {}",
                    store_db_path.display(),
                    error
                );
                return None;
            }
        };
        let mut stmt = match conn.prepare("SELECT data FROM blobs") {
            Ok(stmt) => stmt,
            Err(error) => {
                log::warn!(
                    "failed to query Cursor store DB '{}': {}",
                    store_db_path.display(),
                    error
                );
                return None;
            }
        };
        let rows = match stmt.query_map([], |row| row.get::<_, Vec<u8>>(0)) {
            Ok(rows) => rows,
            Err(error) => {
                log::warn!(
                    "failed to read Cursor store DB rows '{}': {}",
                    store_db_path.display(),
                    error
                );
                return None;
            }
        };

        for row in rows {
            let row = match row {
                Ok(row) => row,
                Err(error) => {
                    log::warn!(
                        "failed to decode Cursor store DB row '{}': {}",
                        store_db_path.display(),
                        error
                    );
                    continue;
                }
            };
            if let Ok(value) = serde_json::from_slice::<Value>(&row) {
                if let Some(content) = value.get("content").and_then(Value::as_str) {
                    if let Some(path) =
                        crate::providers::cursor::tools::extract_workspace_path(content)
                    {
                        return Some(path);
                    }
                }
            }

            let text = String::from_utf8_lossy(&row);
            if let Some(path) =
                crate::providers::cursor::tools::extract_workspace_path(text.as_ref())
            {
                return Some(path);
            }
        }

        None
    }

    fn apply_store_workspace_path(
        &self,
        session: &mut ParsedSession,
        cli_stores: &HashMap<String, PathBuf>,
    ) {
        let store_session_id = session
            .meta
            .parent_id
            .as_deref()
            .unwrap_or(&session.meta.id);
        let Some(store_db_path) = cli_stores.get(store_session_id) else {
            return;
        };
        let Some(project_path) = self.extract_workspace_path_from_store_db(store_db_path) else {
            return;
        };

        session.meta.project_name = project_name_from_path(&project_path);
        session.meta.project_path = project_path;
    }
}

impl SessionProvider for CursorProvider {
    fn provider(&self) -> Provider {
        Provider::Cursor
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        let projects = self.projects_dir();
        if !projects.exists() {
            return Vec::new();
        }

        // Watch each agent-transcripts directory individually
        let mut paths = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&projects) {
            for entry in entries.flatten() {
                let transcripts_dir = entry.path().join("agent-transcripts");
                if transcripts_dir.is_dir() {
                    paths.push(transcripts_dir);
                }
            }
        }
        paths
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let cli_stores = self.collect_cli_session_stores();
        let cli_ids: HashSet<&str> = cli_stores.keys().map(String::as_str).collect();
        let (main_files, subagent_files) = self.collect_transcript_files();

        let mut sessions: Vec<ParsedSession> = main_files
            .par_iter()
            .filter_map(|path| {
                let mut session = self.parse_transcript_jsonl(path)?;
                if !cli_ids.contains(session.meta.id.as_str()) {
                    return None; // IDE session — skip
                }
                self.apply_store_workspace_path(&mut session, &cli_stores);
                Some(session)
            })
            .collect();

        let cli_main_ids: HashSet<&str> = sessions.iter().map(|s| s.meta.id.as_str()).collect();

        let subagent_sessions: Vec<ParsedSession> = subagent_files
            .par_iter()
            .filter_map(|path| {
                let mut session = self.parse_transcript_jsonl(path)?;
                let parent_is_cli = session
                    .meta
                    .parent_id
                    .as_deref()
                    .is_some_and(|pid| cli_main_ids.contains(pid));
                if !parent_is_cli {
                    return None; // parent is IDE — skip subagent too
                }
                self.apply_store_workspace_path(&mut session, &cli_stores);
                Some(session)
            })
            .collect();
        sessions.extend(subagent_sessions);

        Ok(sessions)
    }

    fn deletion_plan(&self, meta: &SessionMeta, children: &[SessionMeta]) -> DeletionPlan {
        if meta.parent_id.is_some() {
            return DeletionPlan {
                file_action: FileAction::Remove,
                child_plans: Vec::new(),
                cleanup_dirs: Vec::new(),
            };
        }

        let child_plans: Vec<ChildPlan> = children
            .iter()
            .map(|c| ChildPlan {
                id: c.id.clone(),
                source_path: c.source_path.clone(),
                title: c.title.clone(),
                file_action: FileAction::Remove,
            })
            .collect();

        // Don't include store.db dir (~/.cursor/chats/<hash>/<sessionId>/) in
        // cleanup_dirs — it's needed for CLI session identification on restore.
        // Only clean up the transcript session directory (subagents/ already moved).
        let source = PathBuf::from(&meta.source_path);
        let cleanup_dirs: Vec<PathBuf> = source
            .parent()
            .filter(|d| d.is_dir())
            .map(|d| d.to_path_buf())
            .into_iter()
            .collect();

        DeletionPlan {
            file_action: FileAction::Remove,
            child_plans,
            cleanup_dirs,
        }
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<LoadedSession, ProviderError> {
        Ok(LoadedSession::new(
            self.load_transcript_messages(source_path)?,
        ))
    }

    fn cleanup_on_permanent_delete(&self, session_id: &str) {
        // Remove store.db directory under ~/.cursor/chats/<hash>/<sessionId>/
        let chats_dir = self.chats_dir();
        if !chats_dir.exists() {
            return;
        }
        let Ok(entries) = std::fs::read_dir(&chats_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let candidate = entry.path().join(session_id);
            if candidate.is_dir() {
                let _ = std::fs::remove_dir_all(&candidate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CursorProvider;
    use crate::provider::SessionProvider;
    use rusqlite::Connection;
    use serde_json::json;

    fn write_main_transcript(
        home: &std::path::Path,
        project_key: &str,
        session_id: &str,
        body: &str,
    ) -> std::path::PathBuf {
        let transcript_dir = home
            .join(".cursor")
            .join("projects")
            .join(project_key)
            .join("agent-transcripts")
            .join(session_id);
        std::fs::create_dir_all(&transcript_dir).unwrap();
        let path = transcript_dir.join(format!("{session_id}.jsonl"));
        std::fs::write(&path, body).unwrap();
        path
    }

    fn write_store_db(home: &std::path::Path, session_id: &str, workspace_path: &str) {
        let store_dir = home
            .join(".cursor")
            .join("chats")
            .join("hash123")
            .join(session_id);
        std::fs::create_dir_all(&store_dir).unwrap();
        let db_path = store_dir.join("store.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute("CREATE TABLE blobs (id TEXT PRIMARY KEY, data BLOB)", [])
            .unwrap();
        conn.execute(
            "INSERT INTO blobs (id, data) VALUES (?1, ?2)",
            (
                "blob1",
                serde_json::to_vec(&json!({
                    "role": "user",
                    "content": format!(
                        "<user_info>\nWorkspace Path: {workspace_path}\n</user_info>"
                    ),
                }))
                .unwrap(),
            ),
        )
        .unwrap();
    }

    #[test]
    fn scan_all_prefers_workspace_path_from_store_db_for_sanitized_project_keys() {
        let dir = tempfile::TempDir::new().unwrap();
        let session_id = "cursor-session-001";
        let workspace_dir = dir
            .path()
            .join("tmp")
            .join(".hidden-root")
            .join("ccsession-real-hidden-cursor");
        std::fs::create_dir_all(&workspace_dir).unwrap();

        write_main_transcript(
            dir.path(),
            "private-tmp-ccsession-real-hidden-cursor",
            session_id,
            r#"{"role":"user","message":{"content":[{"type":"text","text":"<user_query>Reply with OK only.</user_query>"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"OK"}]}}"#,
        );
        write_store_db(
            dir.path(),
            session_id,
            workspace_dir.to_string_lossy().as_ref(),
        );

        let provider = CursorProvider {
            home_dir: dir.path().to_path_buf(),
        };

        let sessions = provider.scan_all().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].meta.project_path,
            workspace_dir.to_string_lossy().to_string()
        );
        assert_eq!(
            sessions[0].meta.project_name,
            "ccsession-real-hidden-cursor"
        );
    }
}
