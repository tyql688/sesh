mod chat_parser;
mod images;
mod logs_parser;
mod orphan;
mod tools;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use crate::models::{Message, Provider, SessionMeta};
use crate::provider::{DeletionPlan, FileAction, ParsedSession, ProviderError, SessionProvider};
use crate::trash_state::active_shared_deletions_by_source;

pub struct Descriptor;
impl crate::provider::ProviderDescriptor for Descriptor {
    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/.gemini/tmp/")
    }
    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("gemini --resume {session_id}"))
    }
    fn display_key(&self, _variant_name: Option<&str>) -> String {
        "gemini".into()
    }
    fn sort_order(&self) -> u32 {
        3
    }
    fn color(&self) -> &'static str {
        "#f59e0b"
    }
    fn cli_command(&self) -> &'static str {
        "gemini"
    }
    fn avatar_svg(&self) -> &'static str {
        r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="#3186FF"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-0)"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-1)"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-2)"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-0" x1="7" x2="11" y1="15.5" y2="12"><stop stop-color="#08B962"/><stop offset="1" stop-color="#08B962" stop-opacity="0"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-1" x1="8" x2="11.5" y1="5.5" y2="11"><stop stop-color="#F94543"/><stop offset="1" stop-color="#F94543" stop-opacity="0"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-2" x1="3.5" x2="17.5" y1="13.5" y2="12"><stop stop-color="#FABC12"/><stop offset=".46" stop-color="#FABC12" stop-opacity="0"/></linearGradient></defs></svg>"##
    }
}

use orphan::chat_session_ids;

pub struct GeminiProvider {
    home_dir: PathBuf,
}

#[derive(Deserialize)]
struct ProjectsFile {
    projects: HashMap<String, String>,
}

#[derive(Deserialize)]
struct LogEntry {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    message: Option<String>,
    timestamp: Option<String>,
}

#[derive(Deserialize)]
struct ChatSession {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "startTime")]
    start_time: Option<String>,
    #[serde(rename = "lastUpdated")]
    last_updated: Option<String>,
    kind: Option<String>,
    messages: Vec<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    msg_type: Option<String>,
    content: Option<serde_json::Value>, // Can be string OR array of {text/inlineData}
    #[serde(rename = "displayContent")]
    display_content: Option<serde_json::Value>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Vec<serde_json::Value>>,
    thoughts: Option<Vec<serde_json::Value>>,
    tokens: Option<serde_json::Value>,
    model: Option<String>,
}

impl GeminiProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    /// Public wrapper for tests: parse a chat JSON file with an explicit project_map.
    pub fn parse_chat_file_for_test(
        &self,
        path: &PathBuf,
        project_map: &HashMap<String, String>,
    ) -> Option<crate::provider::ParsedSession> {
        self.parse_chat_file(path, "", project_map)
    }

    fn gemini_dir(&self) -> PathBuf {
        self.home_dir.join(".gemini")
    }

    fn tmp_dir(&self) -> PathBuf {
        self.gemini_dir().join("tmp")
    }

    fn projects_json_path(&self) -> PathBuf {
        self.gemini_dir().join("projects.json")
    }

    /// Reads projects.json and returns HashMap<project_id, project_path> (reversed from file).
    fn build_project_map(&self) -> HashMap<String, String> {
        let path = self.projects_json_path();
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let projects_file: ProjectsFile = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(_) => return HashMap::new(),
        };

        // File has {"/abs/path": "project-id"}, we reverse to {project-id: "/abs/path"}
        let mut map = HashMap::new();
        for (project_path, project_id) in projects_file.projects {
            map.insert(project_id, project_path);
        }
        map
    }

    fn scan_impl(&self, since_millis: Option<i64>) -> Result<Vec<ParsedSession>, ProviderError> {
        use std::time::{Duration, UNIX_EPOCH};

        let tmp_dir = self.tmp_dir();
        if !tmp_dir.exists() {
            return Ok(Vec::new());
        }

        let project_map = self.build_project_map();
        let shared_deletions = active_shared_deletions_by_source();
        let mut all_sessions = Vec::new();

        let entries: Vec<_> = fs::read_dir(&tmp_dir)?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_dir())
            .collect();

        // Collect sessions indexed by session_id so chat files can replace log entries
        let mut session_map: HashMap<String, ParsedSession> = HashMap::new();

        for entry in entries {
            let project_id = entry.file_name().to_string_lossy().to_string();

            // Parse logs.json
            let logs_path = entry.path().join("logs.json");
            if logs_path.exists() {
                let skip_logs = if let Some(millis) = since_millis {
                    let threshold = UNIX_EPOCH + Duration::from_millis(millis as u64);
                    fs::metadata(&logs_path)
                        .and_then(|m| m.modified())
                        .map(|mtime| mtime < threshold)
                        .unwrap_or(false)
                } else {
                    false
                };

                if !skip_logs {
                    let hidden_ids = shared_deletions
                        .get(&logs_path.to_string_lossy().to_string())
                        .cloned()
                        .unwrap_or_default();
                    let sessions = self
                        .parse_logs_json(&project_id, &logs_path, &project_map)
                        .into_iter()
                        .filter(|session| !hidden_ids.contains(&session.meta.id))
                        .collect::<Vec<_>>();
                    for s in sessions {
                        session_map.insert(s.meta.id.clone(), s);
                    }
                }
            }

            // Parse chats/session-*.json (richer data, replaces logs.json entries)
            let chats_dir = entry.path().join("chats");
            if chats_dir.is_dir() {
                if let Ok(chat_entries) = fs::read_dir(&chats_dir) {
                    for chat_entry in chat_entries.filter_map(std::result::Result::ok) {
                        let chat_path = chat_entry.path();
                        let fname = chat_path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        if !fname.starts_with("session-") || !fname.ends_with(".json") {
                            continue;
                        }

                        // Skip files not modified since threshold
                        if let Some(millis) = since_millis {
                            let threshold = UNIX_EPOCH + Duration::from_millis(millis as u64);
                            let skip = fs::metadata(&chat_path)
                                .and_then(|m| m.modified())
                                .map(|mtime| mtime < threshold)
                                .unwrap_or(false);
                            if skip {
                                continue;
                            }
                        }

                        if let Some(parsed) =
                            self.parse_chat_file(&chat_path, &project_id, &project_map)
                        {
                            // Always replace: chat files have richer data than logs.json
                            session_map.insert(parsed.meta.id.clone(), parsed);
                        }
                    }
                }
            }
        }

        all_sessions.extend(session_map.into_values());
        Ok(all_sessions)
    }
}

impl SessionProvider for GeminiProvider {
    fn provider(&self) -> Provider {
        Provider::Gemini
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.tmp_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        self.scan_impl(None)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name == "logs.json" {
            let project_dir = path.parent().map(PathBuf::from);
            let chat_ids = chat_session_ids(project_dir.as_ref());

            // When chat files exist for this project, they have the full conversation
            // data. Skip logs.json entirely to avoid creating orphan sessions that
            // show incomplete data during live watch.
            if !chat_ids.is_empty() {
                return Ok(Vec::new());
            }

            let project_id = path
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let hidden_ids = active_shared_deletions_by_source()
                .remove(source_path)
                .unwrap_or_default();

            return Ok(self
                .parse_logs_json(&project_id, &path, &project_map)
                .into_iter()
                .filter(|session| !hidden_ids.contains(&session.meta.id))
                .collect());
        }

        if file_name.starts_with("session-") && file_name.ends_with(".json") {
            let project_id = path
                .parent()
                .and_then(|parent| parent.parent())
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();

            return Ok(self
                .parse_chat_file(&path, &project_id, &project_map)
                .into_iter()
                .collect());
        }

        Ok(Vec::new())
    }

    fn load_messages(
        &self,
        session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();

        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Chat files: .gemini/tmp/<project_id>/chats/session-*.json
        if fname.starts_with("session-") && fname.ends_with(".json") {
            let project_id = path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let parsed = self
                .parse_chat_file(&path, &project_id, &project_map)
                .ok_or_else(|| {
                    ProviderError::Parse(format!("session {session_id} not found in {source_path}"))
                })?;

            return Ok(parsed.messages);
        }

        // Fallback: logs.json path -- .gemini/tmp/<project_id>/logs.json
        let project_id = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let sessions = self.parse_logs_json(&project_id, &path, &project_map);

        let session = sessions
            .into_iter()
            .find(|s| s.meta.id == session_id)
            .ok_or_else(|| {
                ProviderError::Parse(format!("session {session_id} not found in {source_path}"))
            })?;

        Ok(session.messages)
    }

    fn deletion_plan(&self, meta: &SessionMeta, _children: &[SessionMeta]) -> DeletionPlan {
        let is_shared = meta.source_path.ends_with("/logs.json");
        DeletionPlan {
            file_action: if is_shared {
                FileAction::Shared
            } else {
                FileAction::Remove
            },
            child_plans: Vec::new(),
            cleanup_dirs: Vec::new(),
        }
    }

    fn purge_from_source(
        &self,
        _source_path: &str,
        _session_id: &str,
    ) -> Result<(), ProviderError> {
        // Gemini logs.json: session removal is handled by shared_deletions mechanism
        Ok(())
    }
}
