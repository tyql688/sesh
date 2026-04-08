// Currently only CLI sessions are processed. IDE (Composer) sessions share the
// same agent-transcripts JSONL format but lack a corresponding store.db under
// ~/.cursor/chats/. We use store.db existence as the hard signal: if a session
// ID has a store.db, it is CLI; otherwise it is IDE and gets skipped entirely.

mod parser;
mod tools;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::models::{Message, Provider, SessionMeta};
use crate::provider::{
    ChildPlan, DeletionPlan, FileAction, ParsedSession, ProviderError, SessionProvider,
};
use rayon::prelude::*;

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

    /// Scan ~/.cursor/chats/<hash>/<sessionId>/store.db to collect all session
    /// IDs that have a store.db file. These are CLI sessions.
    fn collect_cli_session_ids(&self) -> HashSet<String> {
        let chats_dir = self.chats_dir();
        let mut ids = HashSet::new();
        if !chats_dir.exists() {
            return ids;
        }
        let Ok(hash_entries) = std::fs::read_dir(&chats_dir) else {
            return ids;
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
                if session_dir.is_dir() && session_dir.join("store.db").exists() {
                    if let Some(name) = session_dir.file_name().and_then(|n| n.to_str()) {
                        ids.insert(name.to_string());
                    }
                }
            }
        }
        ids
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
        ))
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
        let cli_ids = self.collect_cli_session_ids();
        let (main_files, subagent_files) = self.collect_transcript_files();

        let mut sessions: Vec<ParsedSession> = main_files
            .par_iter()
            .filter_map(|path| {
                let s = self.parse_transcript_jsonl(path)?;
                if !cli_ids.contains(&s.meta.id) {
                    return None; // IDE session — skip
                }
                Some(s)
            })
            .collect();

        let cli_main_ids: HashSet<&str> = sessions.iter().map(|s| s.meta.id.as_str()).collect();

        let subagent_sessions: Vec<ParsedSession> = subagent_files
            .par_iter()
            .filter_map(|path| {
                let s = self.parse_transcript_jsonl(path)?;
                let parent_is_cli = s
                    .meta
                    .parent_id
                    .as_deref()
                    .is_some_and(|pid| cli_main_ids.contains(pid));
                if !parent_is_cli {
                    return None; // parent is IDE — skip subagent too
                }
                Some(s)
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
    ) -> Result<Vec<Message>, ProviderError> {
        self.load_transcript_messages(source_path)
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
