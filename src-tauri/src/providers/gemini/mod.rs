mod chat_parser;
mod images;
mod tools;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use walkdir::WalkDir;

use crate::models::{Message, Provider, SessionMeta};
use crate::provider::{DeletionPlan, FileAction, ParsedSession, ProviderError, SessionProvider};

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
    fn watch_strategy(&self) -> crate::provider::WatchStrategy {
        crate::provider::WatchStrategy::Poll
    }
    fn avatar_svg(&self) -> &'static str {
        r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="#3186FF"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-0)"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-1)"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-2)"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-0" x1="7" x2="11" y1="15.5" y2="12"><stop stop-color="#08B962"/><stop offset="1" stop-color="#08B962" stop-opacity="0"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-1" x1="8" x2="11.5" y1="5.5" y2="11"><stop stop-color="#F94543"/><stop offset="1" stop-color="#F94543" stop-opacity="0"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-2" x1="3.5" x2="17.5" y1="13.5" y2="12"><stop stop-color="#FABC12"/><stop offset=".46" stop-color="#FABC12" stop-opacity="0"/></linearGradient></defs></svg>"##
    }
}

pub struct GeminiProvider {
    home_dir: PathBuf,
}

#[derive(Deserialize)]
struct ProjectsFile {
    projects: HashMap<String, String>,
}

#[derive(Deserialize)]
pub(crate) struct ChatSession {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(rename = "startTime")]
    pub start_time: Option<String>,
    #[serde(rename = "lastUpdated")]
    pub last_updated: Option<String>,
    pub kind: Option<String>,
    pub messages: Vec<ChatMessage>,
}

#[derive(Deserialize)]
pub(crate) struct ChatMessage {
    pub id: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub content: Option<serde_json::Value>,
    #[serde(rename = "displayContent")]
    pub display_content: Option<serde_json::Value>,
    #[serde(rename = "toolCalls")]
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub thoughts: Option<Vec<serde_json::Value>>,
    pub tokens: Option<serde_json::Value>,
    pub model: Option<String>,
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
    ) -> Vec<ParsedSession> {
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

        let mut map = HashMap::new();
        for (project_path, project_id) in projects_file.projects {
            map.insert(project_id, project_path);
        }
        map
    }

    /// Collect all chat JSON files under tmp_dir.
    fn collect_chat_files(&self) -> Vec<(PathBuf, String)> {
        let tmp_dir = self.tmp_dir();
        if !tmp_dir.exists() {
            return Vec::new();
        }

        let mut results = Vec::new();
        for entry in WalkDir::new(&tmp_dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str());
            if ext != Some("json") {
                continue;
            }

            // Must be inside a chats/ directory
            let mut in_chats = false;
            for ancestor in path.ancestors().skip(1) {
                if ancestor.file_name().and_then(|n| n.to_str()) == Some("chats") {
                    in_chats = true;
                    break;
                }
                // Stop at tmp_dir level
                if ancestor == tmp_dir {
                    break;
                }
            }
            if !in_chats {
                continue;
            }

            // Derive project_id from the directory structure:
            // tmp_dir/{project_id}/chats/...
            let relative = path.strip_prefix(&tmp_dir).unwrap_or(path);
            let project_id = relative
                .components()
                .next()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .unwrap_or_default();

            results.push((path.to_path_buf(), project_id));
        }
        results
    }

    fn scan_impl(&self, since_millis: Option<i64>) -> Result<Vec<ParsedSession>, ProviderError> {
        use std::time::{Duration, UNIX_EPOCH};

        let project_map = self.build_project_map();
        let chat_files = self.collect_chat_files();

        let mut all_sessions = Vec::new();

        for (chat_path, project_id) in chat_files {
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

            let parsed = self.parse_chat_file(&chat_path, &project_id, &project_map);
            all_sessions.extend(parsed);
        }

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

        if path.extension().is_none_or(|e| e != "json") {
            return Ok(Vec::new());
        }

        // Derive project_id from path: .../tmp/{project_id}/chats/...
        let tmp_dir = self.tmp_dir();
        let project_id = path
            .strip_prefix(&tmp_dir)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(self.parse_chat_file(&path, &project_id, &project_map))
    }

    fn load_messages(
        &self,
        session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();

        let tmp_dir = self.tmp_dir();
        let project_id = path
            .strip_prefix(&tmp_dir)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .unwrap_or_default();

        let sessions = self.parse_chat_file(&path, &project_id, &project_map);

        let session = sessions
            .into_iter()
            .find(|s| s.meta.id == session_id)
            .ok_or_else(|| {
                ProviderError::Parse(format!("session {session_id} not found in {source_path}"))
            })?;

        Ok(session.messages)
    }

    fn deletion_plan(&self, _meta: &SessionMeta, _children: &[SessionMeta]) -> DeletionPlan {
        // Each chat file is one session, no children for now
        DeletionPlan {
            file_action: FileAction::Remove,
            child_plans: Vec::new(),
            cleanup_dirs: Vec::new(),
        }
    }
}
