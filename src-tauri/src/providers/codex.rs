use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use walkdir::WalkDir;

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT,
};

pub struct CodexProvider {
    home_dir: PathBuf,
}

#[derive(Deserialize)]
struct CodexLine {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    line_type: String,
    payload: Option<Value>,
}

impl CodexProvider {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().expect("cannot resolve HOME directory — app cannot function without it");
        Self { home_dir }
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home_dir.join(".codex").join("sessions")
    }

    fn archived_sessions_dir(&self) -> PathBuf {
        self.home_dir.join(".codex").join("archived_sessions")
    }

    fn collect_jsonl_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in [self.sessions_dir(), self.archived_sessions_dir()] {
            if !dir.exists() {
                continue;
            }
            for entry in WalkDir::new(&dir).into_iter().filter_map(std::result::Result::ok) {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    files.push(path.to_path_buf());
                }
            }
        }
        files
    }

    fn parse_session_file(&self, path: &PathBuf) -> Option<ParsedSession> {
        let file = File::open(path).ok()?;
        let metadata = fs::metadata(path).ok()?;
        let file_size = metadata.len();

        let reader = BufReader::new(file);
        let mut messages = Vec::new();
        let mut first_user_message: Option<String> = None;
        let mut first_timestamp: Option<String> = None;
        let mut last_timestamp: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut session_id: Option<String> = None;
        let mut cwd: Option<String> = None;
        // Map call_id -> message index for merging function_call_output into function_call
        let mut call_id_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };
            if line.trim().is_empty() {
                continue;
            }

            let entry: CodexLine = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if let Some(ref ts) = entry.timestamp {
                if first_timestamp.is_none() {
                    first_timestamp = Some(ts.clone());
                }
                last_timestamp = Some(ts.clone());
            }

            let payload = match entry.payload {
                Some(ref p) => p,
                None => continue,
            };

            match entry.line_type.as_str() {
                "session_meta" => {
                    if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                        session_id = Some(id.to_string());
                    }
                    if let Some(c) = payload.get("cwd").and_then(|v| v.as_str()) {
                        cwd = Some(c.to_string());
                    }
                }
                "response_item" => {
                    // Skip developer role and reasoning type
                    let role_str = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let item_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

                    if role_str == "developer" || item_type == "reasoning" {
                        continue;
                    }

                    match item_type {
                        "message" => {
                            let text = extract_codex_content(payload);
                            let normalized_text = strip_inline_image_sources(&text);
                            let role = match role_str {
                                "user" => MessageRole::User,
                                "assistant" => MessageRole::Assistant,
                                _ => continue,
                            };

                            // Skip empty messages and system/environment XML content
                            if text.is_empty() {
                                continue;
                            }
                            let trimmed = normalized_text.trim_start();
                            if is_system_content(trimmed) {
                                continue;
                            }

                            if role == MessageRole::User && first_user_message.is_none() {
                                first_user_message = Some(normalized_text.clone());
                            }

                            if !normalized_text.is_empty() {
                                content_parts.push(normalized_text);
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
                        "function_call" => {
                            let raw_name = payload
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let arguments_str = payload.get("arguments").and_then(|v| v.as_str());

                            // Map Codex tool names to our display names
                            let display_name = map_codex_tool_name(raw_name);

                            // For exec_command, remap arguments to match Bash tool format
                            let tool_input = match raw_name {
                                "exec_command" => {
                                    // Remap {"cmd": "..."} to {"command": "..."}
                                    arguments_str.and_then(|s| {
                                        let v: Value = serde_json::from_str(s).ok()?;
                                        let cmd = v.get("cmd").and_then(|c| c.as_str())?;
                                        Some(json!({"command": cmd}).to_string())
                                    })
                                }
                                "view_image" => {
                                    // Emit as image message instead of tool
                                    if let Some(path) = arguments_str
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                        .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()))
                                    {
                                        messages.push(Message {
                                            role: MessageRole::Assistant,
                                            content: format!("[Image: source: {path}]"),
                                            timestamp: entry.timestamp.clone(),
                                            tool_name: None,
                                            tool_input: None,
                                            token_usage: None,
                                        });
                                        continue;
                                    }
                                    None
                                }
                                "write_stdin" => {
                                    // Skip empty stdin writes (just polling)
                                    let is_empty = arguments_str
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                        .and_then(|v| v.get("chars").and_then(|c| c.as_str()).map(|s| s.is_empty()))
                                        .unwrap_or(true);
                                    if is_empty {
                                        continue;
                                    }
                                    arguments_str.map(|s| s.to_string())
                                }
                                _ => arguments_str.map(|s| s.to_string()),
                            };

                            let idx = messages.len();
                            if let Some(cid) = payload.get("call_id").and_then(|v| v.as_str()) {
                                call_id_map.insert(cid.to_string(), idx);
                            }
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: String::new(),
                                timestamp: entry.timestamp.clone(),
                                tool_name: Some(display_name.to_string()),
                                tool_input,
                                token_usage: None,
                            });
                        }
                        "function_call_output" => {
                            let raw_output = match payload.get("output") {
                                Some(Value::String(s)) => s.clone(),
                                Some(other) => serde_json::to_string(other).unwrap_or_default(),
                                None => String::new(),
                            };
                            let output = extract_tool_output(&raw_output);

                            if !output.is_empty() {
                                content_parts.push(output.clone());
                            }

                            // Merge output into the matching function_call message
                            let call_id = payload.get("call_id").and_then(|v| v.as_str());
                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied() {
                                if idx < messages.len() {
                                    messages[idx].content = output;
                                    continue;
                                }
                            }
                            // Fallback: standalone output message
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: output,
                                timestamp: entry.timestamp.clone(),
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                            });
                        }
                        "custom_tool_call" => {
                            let raw_name = payload
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("tool");
                            let display_name = map_codex_tool_name(raw_name);
                            let input = payload
                                .get("input")
                                .map(|v| {
                                    if let Some(s) = v.as_str() {
                                        s.to_string()
                                    } else {
                                        serde_json::to_string(v).unwrap_or_default()
                                    }
                                });

                            let idx = messages.len();
                            if let Some(cid) = payload.get("call_id").and_then(|v| v.as_str()) {
                                call_id_map.insert(cid.to_string(), idx);
                            }
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: String::new(),
                                timestamp: entry.timestamp.clone(),
                                tool_name: Some(display_name.to_string()),
                                tool_input: input,
                                token_usage: None,
                            });
                        }
                        "custom_tool_call_output" => {
                            let raw_output = payload
                                .get("output")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let output = extract_tool_output(&raw_output);

                            let call_id = payload.get("call_id").and_then(|v| v.as_str());
                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied() {
                                if idx < messages.len() {
                                    messages[idx].content = output;
                                    continue;
                                }
                            }
                            if !output.is_empty() {
                                messages.push(Message {
                                    role: MessageRole::Tool,
                                    content: output,
                                    timestamp: entry.timestamp.clone(),
                                    tool_name: None,
                                    tool_input: None,
                                    token_usage: None,
                                });
                            }
                        }
                        _ => continue,
                    }
                }
                "event_msg" => {
                    let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    // agent_message is a duplicate of response_item/message/assistant — skip
                    if event_type == "token_count" {
                        if let Some(info) = payload.get("info") {
                            if let Some(last) = info.get("last_token_usage") {
                                let input = last.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let output = last.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let cached = last.get("cached_input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                                let usage = TokenUsage {
                                    input_tokens: input,
                                    output_tokens: output,
                                    cache_read_input_tokens: cached,
                                    cache_creation_input_tokens: 0,
                                };
                                if let Some(last_msg) = messages.iter_mut().rev()
                                    .find(|m| m.role == MessageRole::Assistant)
                                {
                                    last_msg.token_usage = Some(usage);
                                }
                            }
                        }
                    }
                }
                _ => continue,
            }
        }

        if messages.is_empty() {
            return None;
        }

        // Session ID: from session_meta payload.id, fallback to filename parsing
        let session_id = session_id.unwrap_or_else(|| {
            path.file_stem().map_or_else(|| "unknown".to_string(), |s| s.to_string_lossy().to_string())
        });

        let title = session_title(first_user_message.as_deref());

        let project_path = cwd.unwrap_or_else(|| NO_PROJECT.to_string());

        let project_name = project_name_from_path(&project_path);

        let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

        let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

        let full_content = content_parts.join("\n");
        let content_text = truncate_to_bytes(&full_content, 2000);

        let meta = SessionMeta {
            id: session_id,
            provider: Provider::Codex,
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

fn extract_codex_content(payload: &Value) -> String {
    match payload.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => extract_codex_array_content(arr),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => {
            // Also check for direct "output" field (function_call_output)
            payload
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
    }
}

fn extract_codex_array_content(arr: &[Value]) -> String {
    let mut parts = Vec::new();

    for item in arr {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match item_type {
            "input_image" => {
                if let Some(image_url) = item.get("image_url").and_then(|v| v.as_str()) {
                    parts.push(format!("[Image: source: {image_url}]"));
                }
            }
            _ => {
                let Some(text) = extract_codex_text(item) else {
                    continue;
                };

                if is_codex_image_wrapper(text) {
                    continue;
                }

                parts.push(text.to_string());
            }
        }
    }

    parts.join("\n")
}

fn extract_codex_text(item: &Value) -> Option<&str> {
    item.get("text")
        .or_else(|| item.get("output_text"))
        .or_else(|| item.get("input_text"))
        .and_then(|t| t.as_str())
}

fn is_codex_image_wrapper(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with("<image name=") && trimmed.ends_with('>')) || trimmed == "</image>"
}

/// Extract readable text from Codex tool output.
/// Handles: plain text, JSON `{"output":"..."}`, JSON array `[{"type":"text","text":"..."}]`.
fn extract_tool_output(raw: &str) -> String {
    let trimmed = raw.trim();
    // Try JSON object with "output" field (custom_tool_call_output)
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if let Some(out) = v.get("output").and_then(|o| o.as_str()) {
                return out.to_string();
            }
        }
    }
    // Try JSON array of text parts (MCP tool output)
    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<Value>>(trimmed) {
            let texts: Vec<&str> = arr
                .iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
    }
    raw.to_string()
}

/// Map Codex function names to display names matching our UI conventions.
fn map_codex_tool_name(name: &str) -> &str {
    match name {
        "exec_command" => "Bash",
        "apply_patch" => "Apply_patch",
        "view_image" => "Image",
        "update_plan" => "Plan",
        "write_stdin" => "Stdin",
        _ if name.starts_with("mcp__") => {
            // e.g. mcp__playwright__browser_click -> last segment
            name.rsplit("__").next().unwrap_or(name)
        }
        _ => name,
    }
}

fn strip_inline_image_sources(text: &str) -> String {
    if !text.contains("[Image: source:") {
        return text.to_string();
    }

    text.lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("[Image: source:") {
                "[Image]".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl SessionProvider for CodexProvider {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.sessions_dir(), self.archived_sessions_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let files = self.collect_jsonl_files();
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let sessions: Vec<ParsedSession> = files
            .par_iter()
            .filter_map(|path| self.parse_session_file(path))
            .collect();

        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        Ok(self.parse_session_file(&path).into_iter().collect())
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);

        let parsed = self.parse_session_file(&path).ok_or_else(|| {
            ProviderError::Parse("failed to parse codex session file".to_string())
        })?;

        Ok(parsed.messages)
    }
}
