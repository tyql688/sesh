use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    parse_rfc3339_timestamp, project_name_from_path, session_title, truncate_to_bytes,
    FTS_CONTENT_LIMIT, NO_PROJECT,
};

use super::orphan::{collect_real_session_prefixes, merge_orphan_sessions};
use super::tools::normalize_gemini_message;
use super::{GeminiProvider, LogEntry};

impl GeminiProvider {
    pub(super) fn parse_logs_json(
        &self,
        project_id: &str,
        path: &PathBuf,
        project_map: &HashMap<String, String>,
    ) -> Vec<ParsedSession> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("cannot read Gemini logs '{}': {}", path.display(), e);
                return Vec::new();
            }
        };

        let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let entries: Vec<LogEntry> = match serde_json::from_str(&content) {
            Ok(e) => e,
            Err(_) => return Vec::new(),
        };

        // Group entries by sessionId
        let mut groups: HashMap<String, Vec<&LogEntry>> = HashMap::new();
        for entry in &entries {
            groups
                .entry(entry.session_id.clone())
                .or_default()
                .push(entry);
        }

        let project_path = project_map
            .get(project_id)
            .cloned()
            .unwrap_or_else(|| NO_PROJECT.to_string());

        let project_name = project_name_from_path(&project_path);

        let mut sessions = Vec::new();

        for (session_id, group) in &groups {
            let mut messages = Vec::new();
            let mut first_user_message: Option<String> = None;
            let mut first_timestamp: Option<String> = None;
            let mut last_timestamp: Option<String> = None;
            let mut content_parts: Vec<String> = Vec::new();

            for entry in group {
                if let Some(ref ts) = entry.timestamp {
                    if first_timestamp.is_none() {
                        first_timestamp = Some(ts.clone());
                    }
                    last_timestamp = Some(ts.clone());
                }

                let role = match entry.entry_type.as_deref() {
                    Some("user") => MessageRole::User,
                    Some("assistant") => MessageRole::Assistant,
                    _ => MessageRole::User,
                };

                let text = normalize_gemini_message(
                    entry.message.as_deref().unwrap_or_default(),
                    &project_path,
                );

                if role == MessageRole::User && first_user_message.is_none() && !text.is_empty() {
                    first_user_message = Some(text.clone());
                }

                if !text.is_empty() {
                    content_parts.push(text.clone());
                }

                messages.push(Message {
                    role,
                    content: text,
                    timestamp: entry.timestamp.clone(),
                    tool_name: None,
                    tool_input: None,
                    token_usage: None,
                    model: None,
                });
            }

            if messages.is_empty() {
                continue;
            }

            let title = session_title(first_user_message.as_deref());

            let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

            let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

            let full_content = content_parts.join("\n");
            let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);

            let meta = SessionMeta {
                id: session_id.clone(),
                provider: Provider::Gemini,
                title,
                project_path: project_path.clone(),
                project_name: project_name.clone(),
                created_at,
                updated_at,
                message_count: messages.len() as u32,
                file_size_bytes: file_size,
                source_path: path.to_string_lossy().to_string(),
                is_sidechain: false,
                variant_name: None,
                model: None,
                cc_version: None,
                git_branch: None,
                parent_id: None,
            };

            sessions.push(ParsedSession {
                meta,
                messages,
                content_text,
            });
        }

        // Merge orphan sessions into real sessions.
        // Gemini CLI sometimes assigns separate sessionIds to image sends.
        // Real sessions have chat files in chats/ directory.
        let project_dir = path.parent().unwrap_or(Path::new(""));
        let real_prefixes = collect_real_session_prefixes(project_dir);
        merge_orphan_sessions(&mut sessions, &real_prefixes);

        sessions
    }
}
