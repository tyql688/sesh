use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT,
};
use crate::trash_state::active_shared_deletions_by_source;

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
    messages: Vec<ChatMessage>,
}

#[derive(Deserialize)]
struct ChatMessage {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    msg_type: Option<String>,
    content: Option<serde_json::Value>, // Can be string OR array of {text/inlineData}
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Vec<serde_json::Value>>,
    thoughts: Option<Vec<serde_json::Value>>,
    tokens: Option<serde_json::Value>,
}

impl GeminiProvider {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().expect("cannot resolve HOME directory — app cannot function without it");
        Self { home_dir }
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

    fn parse_logs_json(
        &self,
        project_id: &str,
        path: &PathBuf,
        project_map: &HashMap<String, String>,
    ) -> Vec<ParsedSession> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warn: cannot read Gemini logs '{}': {}", path.display(), e);
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
                });
            }

            if messages.is_empty() {
                continue;
            }

            let title = session_title(first_user_message.as_deref());

            let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

            let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

            let full_content = content_parts.join("\n");
            let content_text = truncate_to_bytes(&full_content, 2000);

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

    fn parse_chat_file(
        &self,
        path: &PathBuf,
        project_id: &str,
        project_map: &HashMap<String, String>,
    ) -> Option<ParsedSession> {
        let content = fs::read_to_string(path).ok()?;
        let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let chat: ChatSession = serde_json::from_str(&content).ok()?;

        let project_path = project_map
            .get(project_id)
            .cloned()
            .unwrap_or_else(|| NO_PROJECT.to_string());

        let project_name = project_name_from_path(&project_path);

        let mut messages = Vec::new();
        let mut first_user_message: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();

        for msg in &chat.messages {
            let role = match msg.msg_type.as_deref() {
                Some("user") => MessageRole::User,
                Some("model") | Some("gemini") | Some("assistant") => MessageRole::Assistant,
                _ => continue,
            };

            // content can be a string or an array of {text, inlineData}
            let text = match &msg.content {
                Some(serde_json::Value::String(s)) => normalize_gemini_message(s, &project_path),
                Some(serde_json::Value::Array(arr)) => {
                    // If inlineData exists, @path image refs in text are duplicates
                    let has_inline_data = arr.iter().any(|item| item.get("inlineData").is_some());

                    let mut parts = Vec::new();
                    for item in arr {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            // Filter Gemini context markers
                            let trimmed = text.trim();
                            if trimmed.starts_with("--- Content from referenced files ---")
                                || trimmed.starts_with("--- End of content ---")
                                || trimmed.is_empty()
                            {
                                continue;
                            }
                            let normalized = if has_inline_data {
                                // Strip @path image refs, keep only caption text
                                strip_at_image_refs(trimmed)
                            } else {
                                normalize_gemini_message(trimmed, &project_path)
                            };
                            if !normalized.is_empty() {
                                parts.push(normalized);
                            }
                        } else if let Some(inline) = item.get("inlineData") {
                            let mime = inline
                                .get("mimeType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            if let Some(data) = inline.get("data").and_then(|d| d.as_str()) {
                                parts.push(format!(
                                    "[Image: source: data:{mime};base64,{data}]"
                                ));
                            }
                        }
                    }
                    parts.join("\n")
                }
                _ => String::new(),
            };

            if text.is_empty() && msg.tool_calls.is_none() {
                continue;
            }

            let trimmed = text.trim_start();
            if !text.is_empty() && is_system_content(trimmed) {
                continue;
            }

            // Extract token usage for this turn
            let token_usage = msg.tokens.as_ref().and_then(|t| {
                let input = t.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let output = t.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let cached = t.get("cached").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if input == 0 && output == 0 {
                    None
                } else {
                    Some(TokenUsage {
                        input_tokens: input,
                        output_tokens: output,
                        cache_read_input_tokens: cached,
                        cache_creation_input_tokens: 0,
                    })
                }
            });

            // Emit thoughts as [thinking] system messages (before text)
            if role == MessageRole::Assistant {
                if let Some(ref thoughts) = msg.thoughts {
                    for thought in thoughts {
                        let subject = thought.get("subject").and_then(|s| s.as_str()).unwrap_or("");
                        let description = thought.get("description").and_then(|d| d.as_str()).unwrap_or("");
                        if !description.is_empty() {
                            let thinking_ts = thought.get("timestamp").and_then(|t| t.as_str()).map(|s| s.to_string()).or_else(|| msg.timestamp.clone());
                            let content = if subject.is_empty() {
                                format!("[thinking]\n{description}")
                            } else {
                                format!("[thinking]\n**{subject}**\n{description}")
                            };
                            messages.push(Message {
                                role: MessageRole::System,
                                content,
                                timestamp: thinking_ts,
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                            });
                        }
                    }
                }
            }

            if !text.is_empty() {
                if role == MessageRole::User && first_user_message.is_none() {
                    first_user_message = Some(text.clone());
                }

                content_parts.push(text.clone());

                let has_tools = msg.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty());
                messages.push(Message {
                    role: role.clone(),
                    content: text,
                    timestamp: msg.timestamp.clone(),
                    tool_name: None,
                    tool_input: None,
                    // Attach token usage to text msg only if no tool calls follow
                    token_usage: if !has_tools { token_usage.clone() } else { None },
                });
            }

            // Extract tool calls as Tool messages
            if let Some(ref tool_calls) = msg.tool_calls {
                let last_idx = tool_calls.len().saturating_sub(1);
                for (i, tc) in tool_calls.iter().enumerate() {
                    let display_name = tc
                        .get("displayName")
                        .and_then(|n| n.as_str())
                        .or_else(|| tc.get("name").and_then(|n| n.as_str()))
                        .unwrap_or("tool");
                    let name = map_gemini_tool_name(display_name).to_string();

                    // Remap args for Bash: shell_command {command} or run_shell_command {command}
                    let args = match name.as_str() {
                        "Bash" => {
                            tc.get("args").and_then(|a| {
                                let obj = a.as_object()?;
                                let cmd = obj.get("command").or_else(|| obj.get("cmd")).and_then(|c| c.as_str())?;
                                Some(serde_json::json!({"command": cmd}).to_string())
                            }).or_else(|| tc.get("args").map(std::string::ToString::to_string))
                        }
                        "Write" => {
                            tc.get("args").and_then(|a| {
                                let obj = a.as_object()?;
                                let fp = obj.get("file_path").and_then(|f| f.as_str())?;
                                Some(serde_json::json!({"file_path": fp}).to_string())
                            }).or_else(|| tc.get("args").map(std::string::ToString::to_string))
                        }
                        _ => tc.get("args").map(std::string::ToString::to_string),
                    };

                    let result_text = tc
                        .get("result")
                        .and_then(|r| r.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|item| item.get("functionResponse"))
                        .and_then(|fr| fr.get("response"))
                        .and_then(|resp| resp.get("output"))
                        .and_then(|o| o.as_str())
                        .unwrap_or("")
                        .to_string();

                    messages.push(Message {
                        role: MessageRole::Tool,
                        content: result_text,
                        timestamp: msg.timestamp.clone(),
                        tool_name: Some(name),
                        tool_input: args,
                        // Attach token usage to last tool message
                        token_usage: if i == last_idx { token_usage.clone() } else { None },
                    });
                }
            }
        }

        if messages.is_empty() {
            return None;
        }

        let title = session_title(first_user_message.as_deref());

        let created_at = parse_rfc3339_timestamp(chat.start_time.as_deref());

        let updated_at = parse_rfc3339_timestamp(chat.last_updated.as_deref());

        let content_text = truncate_to_bytes(&content_parts.join("\n"), 2000);

        let meta = SessionMeta {
            id: chat.session_id,
            provider: Provider::Gemini,
            title,
            project_path,
            project_name,
            created_at,
            updated_at,
            message_count: messages.len() as u32,
            file_size_bytes: file_size,
            source_path: path.to_string_lossy().to_string(),
            is_sidechain: false,
        };

        Some(ParsedSession {
            meta,
            messages,
            content_text,
        })
    }
}

impl GeminiProvider {
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
                    ProviderError::Parse(format!(
                        "session {session_id} not found in {source_path}"
                    ))
                })?;

            return Ok(parsed.messages);
        }

        // Fallback: logs.json path — .gemini/tmp/<project_id>/logs.json
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
                ProviderError::Parse(format!(
                    "session {session_id} not found in {source_path}"
                ))
            })?;

        Ok(session.messages)
    }
}

fn chat_session_ids(project_dir: Option<&PathBuf>) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    let Some(project_dir) = project_dir else {
        return ids;
    };

    let chats_dir = project_dir.join("chats");
    let Ok(entries) = fs::read_dir(chats_dir) else {
        return ids;
    };

    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        let Some(file_name) = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
        else {
            continue;
        };

        if file_name.starts_with("session-") && file_name.ends_with(".json") {
            ids.insert(
                file_name
                    .trim_start_matches("session-")
                    .trim_end_matches(".json")
                    .to_string(),
            );
        }
    }

    ids
}

/// Collect "real" session ID prefixes from the chats/ directory.
/// Gemini CLI stores real sessions as `chats/session-DATE-IDPREFIX.json`.
/// Sessions NOT in this list are orphans (e.g. image sends with new sessionIds).
fn collect_real_session_prefixes(project_dir: &Path) -> Vec<String> {
    let chats_dir = project_dir.join("chats");
    let mut prefixes = Vec::new();
    if let Ok(entries) = fs::read_dir(&chats_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Format: session-2026-03-27T07-02-63f10611.json
            if let Some(stem) = name.strip_suffix(".json") {
                if let Some(last_dash) = stem.rfind('-') {
                    prefixes.push(stem[last_dash + 1..].to_string());
                }
            }
        }
    }
    // Fallback: check for UUID-named directories (older Gemini versions)
    if prefixes.is_empty() {
        if let Ok(entries) = fs::read_dir(project_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if entry.path().is_dir() && name.len() > 30 && name.contains('-') {
                    prefixes.push(name[..8].to_string());
                }
            }
        }
    }
    prefixes
}

/// Merge orphan sessions (not in chats/) into the nearest real session.
fn merge_orphan_sessions(sessions: &mut Vec<ParsedSession>, real_prefixes: &[String]) {
    if sessions.len() < 2 || real_prefixes.is_empty() {
        return;
    }

    sessions.sort_by_key(|s| s.meta.created_at);

    let is_real = |sid: &str| -> bool {
        real_prefixes.iter().any(|p| sid.starts_with(p))
    };

    // Identify orphan vs real indices
    let orphan_indices: Vec<usize> = sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| !is_real(&s.meta.id))
        .map(|(i, _)| i)
        .collect();

    if orphan_indices.is_empty() {
        return;
    }

    // For each orphan, find nearest real session (prefer preceding)
    let mut merges: Vec<(usize, usize)> = Vec::new();
    for &orphan_idx in &orphan_indices {
        let mut target: Option<usize> = None;
        for j in (0..orphan_idx).rev() {
            if !orphan_indices.contains(&j) {
                target = Some(j);
                break;
            }
        }
        if target.is_none() {
            for j in (orphan_idx + 1)..sessions.len() {
                if !orphan_indices.contains(&j) {
                    target = Some(j);
                    break;
                }
            }
        }
        if let Some(t) = target {
            merges.push((orphan_idx, t));
        }
    }

    // Apply merges: first merge data, then remove orphans
    for &(orphan_idx, target_idx) in &merges {
        if orphan_idx >= sessions.len() || target_idx >= sessions.len() {
            continue;
        }
        let orphan = sessions[orphan_idx].clone();
        let target = &mut sessions[target_idx];
        target.messages.extend(orphan.messages);
        target.meta.message_count = target.messages.len() as u32;
        if orphan.meta.updated_at > target.meta.updated_at {
            target.meta.updated_at = orphan.meta.updated_at;
        }
        if !orphan.content_text.is_empty() {
            target.content_text.push('\n');
            target.content_text.push_str(&orphan.content_text);
        }
    }

    // Remove orphans in reverse index order to preserve indices
    let mut remove_indices: Vec<usize> = merges.iter().map(|(o, _)| *o).collect();
    remove_indices.sort_unstable_by(|a, b| b.cmp(a));
    remove_indices.dedup();
    for idx in remove_indices {
        if idx < sessions.len() {
            sessions.remove(idx);
        }
    }
}

/// Strip `@path/to/image.png` references from text, keeping only non-path caption text.
/// Used when inlineData already provides the image.
fn strip_at_image_refs(text: &str) -> String {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('@') {
                // "@path/to/image.png caption" → keep caption only
                let after_at = trimmed.strip_prefix('@').unwrap_or(trimmed).trim();
                if let Some(space_idx) = after_at.find(|c: char| c.is_whitespace()) {
                    let path_part = &after_at[..space_idx];
                    if looks_like_image_path(path_part) {
                        let rest = after_at[space_idx..].trim();
                        return if rest.is_empty() { None } else { Some(rest.to_string()) };
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

/// Map Gemini tool display names to canonical names.
fn map_gemini_tool_name(display_name: &str) -> &str {
    match display_name {
        "Shell" | "shell" | "run_shell_command" => "Bash",
        "WriteFile" | "write_file" => "Write",
        "ReadFile" | "read_file" => "Read",
        "ReadFolder" | "list_directory" => "Glob",
        "Edit" | "edit_file" => "Edit",
        "Enter Plan Mode" | "enter_plan_mode" => "Plan",
        s if s.contains("Agent") || s.contains("agent") => "Agent",
        _ => display_name,
    }
}

fn normalize_gemini_message(text: &str, project_path: &str) -> String {
    if !text.contains(".png")
        && !text.contains(".jpg")
        && !text.contains(".jpeg")
        && !text.contains(".gif")
        && !text.contains(".webp")
        && !text.contains(".bmp")
    {
        return text.to_string();
    }

    text.lines()
        .map(|line| {
            let trimmed = line.trim();
            // Try the whole line first
            if let Some(image_path) = resolve_gemini_image_path(trimmed, project_path) {
                return format!("[Image: source: {image_path}]");
            }
            // Handle "@ path/to/image.png some caption text" — extract the path token
            let token = trimmed.strip_prefix('@').unwrap_or(trimmed).trim();
            if let Some(space_idx) = token.find(|c: char| c.is_whitespace()) {
                let path_part = &token[..space_idx];
                let rest = token[space_idx..].trim();
                if looks_like_image_path(path_part) {
                    let full_raw = if trimmed.starts_with('@') {
                        format!("@{path_part}")
                    } else {
                        path_part.to_string()
                    };
                    if let Some(image_path) = resolve_gemini_image_path(&full_raw, project_path) {
                        return if rest.is_empty() {
                            format!("[Image: source: {image_path}]")
                        } else {
                            format!("[Image: source: {image_path}]\n{rest}")
                        };
                    }
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn resolve_gemini_image_path(raw: &str, project_path: &str) -> Option<String> {
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
        if let Some(index) = candidate.find(".gemini/") {
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
        let allowed = path_str.starts_with(&*home_str)
            || path_str.starts_with("/tmp/")
            || path_str.starts_with("/private/tmp/")
            || path_str.starts_with("/var/folders/");
        if !allowed {
            return None;
        }
    }

    Some(path_str.to_string())
}

fn looks_like_image_path(candidate: &str) -> bool {
    let lower = candidate.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn normalize_path(path: &Path) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::normalize_gemini_message;

    #[test]
    fn normalize_gemini_message_turns_attachment_refs_into_image_markers() {
        let text = "@../../../.gemini/tmp/demo/images/clipboard-1.png";
        let normalized = normalize_gemini_message(text, "/Users/test/Documents/project/demo");
        assert_eq!(
            normalized,
            "[Image: source: /Users/test/.gemini/tmp/demo/images/clipboard-1.png]"
        );
    }

    #[test]
    fn normalize_gemini_message_handles_image_path_with_trailing_text() {
        let text = "@../../../.gemini/tmp/demo/images/clipboard-1.png 这是什么";
        let normalized = normalize_gemini_message(text, "/Users/test/Documents/project/demo");
        assert_eq!(
            normalized,
            "[Image: source: /Users/test/.gemini/tmp/demo/images/clipboard-1.png]\n这是什么"
        );
    }
}
