use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::Value;
use walkdir::WalkDir;

use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{is_system_content, session_title, truncate_to_bytes};

pub struct CursorProvider {
    home_dir: PathBuf,
}

impl CursorProvider {
    pub fn new() -> Self {
        let home_dir =
            dirs::home_dir().expect("cannot resolve HOME directory — app cannot function without it");
        Self { home_dir }
    }

    fn chats_dir(&self) -> PathBuf {
        self.home_dir.join(".cursor").join("chats")
    }

    fn open_db(db_path: &Path) -> Option<Connection> {
        // Use READ_WRITE to allow reading uncommitted WAL data.
        // SQLITE_OPEN_READ_ONLY cannot reliably read WAL in shared-cache mode.
        let conn = Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        // Ensure WAL reads see latest committed data
        let _ = conn.pragma_update(None, "journal_mode", "wal");
        Some(conn)
    }

    /// Read all blobs as UTF-8 strings, ordered by rowid.
    fn read_blobs(conn: &Connection) -> Vec<String> {
        let mut stmt = match conn.prepare("SELECT data FROM blobs ORDER BY rowid") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            Ok(String::from_utf8_lossy(&bytes).to_string())
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    fn parse_session_db(&self, db_path: &Path) -> Option<ParsedSession> {
        let conn = Self::open_db(db_path)?;
        let rows = Self::read_blobs(&conn);

        let mut first_user_message: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut message_count: u32 = 0;
        let mut project_path = String::new();

        for raw in &rows {
            let msg: Value = match serde_json::from_str(raw) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "user" => {
                    let text = extract_text_content(&msg);
                    // Extract project path from user_info before filtering
                    if project_path.is_empty() {
                        if let Some(wp) = extract_workspace_path(&text) {
                            project_path = wp;
                        }
                    }
                    let clean = extract_user_text(&text);
                    if !clean.is_empty() {
                        if first_user_message.is_none() {
                            first_user_message = Some(clean.clone());
                        }
                        content_parts.push(clean);
                        message_count += 1;
                    }
                }
                "assistant" => {
                    let text = extract_text_content(&msg);
                    let clean = strip_think_tags(&text);
                    if !clean.is_empty() {
                        content_parts.push(clean);
                    }
                    message_count += 1;
                }
                "tool" => {
                    message_count += 1;
                }
                _ => {}
            }
        }

        if message_count == 0 {
            return None;
        }

        let title = session_title(first_user_message.as_deref());
        let project_name = project_path
            .split('/')
            .next_back()
            .unwrap_or("")
            .to_string();
        let file_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

        // Session ID from directory name (UUID)
        let session_id = db_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Approximate timestamps from file metadata
        let created_at = std::fs::metadata(db_path)
            .ok()
            .and_then(|m| m.created().ok())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
            .unwrap_or(0);
        let updated_at = std::fs::metadata(db_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
            .unwrap_or(created_at);

        Some(ParsedSession {
            meta: SessionMeta {
                id: session_id,
                provider: Provider::Cursor,
                title,
                project_path: project_path.clone(),
                project_name,
                created_at,
                updated_at,
                message_count,
                file_size_bytes: file_size,
                source_path: db_path.to_string_lossy().to_string(),
                is_sidechain: false,
            },
            messages: Vec::new(),
            content_text: truncate_to_bytes(&content_parts.join("\n"), 2000),
        })
    }
}

/// Extract clean user text: strip <user_query> tags, filter system content.
fn extract_user_text(text: &str) -> String {
    // Try extracting from <user_query> tags
    if let Some(inner) = extract_tag_content(text, "user_query") {
        let trimmed = inner.trim();
        if !trimmed.is_empty() && !is_system_content(trimmed) {
            return trimmed.to_string();
        }
    }
    // If no user_query tag, check if it's system content
    let trimmed = text.trim();
    if trimmed.is_empty()
        || is_system_content(trimmed)
        || trimmed.starts_with("<user_info>")
        || trimmed.starts_with("<agent_transcripts>")
    {
        return String::new();
    }
    trimmed.to_string()
}

/// Extract content between <tag>...</tag>.
fn extract_tag_content<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let after = &text[start + open.len()..];
    let end = after.find(&close)?;
    Some(&after[..end])
}

/// Extract workspace path from user_info XML in user messages.
fn extract_workspace_path(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Workspace Path:") {
            let path = rest.trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Extract text from message content (string or array of parts).
fn extract_text_content(msg: &Value) -> String {
    extract_text_from_content(msg.get("content"))
}

fn extract_text_from_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => {
            // Content might be a JSON array serialized as string
            if s.trim_start().starts_with('[') {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(s) {
                    return extract_text_from_parts(&arr);
                }
            }
            s.clone()
        }
        Some(Value::Array(arr)) => extract_text_from_parts(arr),
        _ => String::new(),
    }
}

fn extract_text_from_parts(arr: &[Value]) -> String {
    arr.iter()
        .filter_map(|item| {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse content field as array of Value parts.
fn parse_content_array(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::Array(arr)) => arr.clone(),
        Some(Value::String(s)) => {
            if s.trim_start().starts_with('[') {
                serde_json::from_str::<Vec<Value>>(s).unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Strip <think>...</think> tags, return remaining visible text.
fn strip_think_tags(text: &str) -> String {
    if !text.contains("<think>") {
        return text.to_string();
    }
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            result = format!("{}{}", &result[..start], &result[end + "</think>".len()..]);
        } else {
            result = result[..start].to_string();
            break;
        }
    }
    result.trim().to_string()
}

/// Extract <think>...</think> content.
fn extract_think_content(text: &str) -> Option<String> {
    let start = text.find("<think>")?;
    let after = &text[start + "<think>".len()..];
    let end = after.find("</think>").unwrap_or(after.len());
    let thinking = after[..end].trim();
    if thinking.is_empty() {
        None
    } else {
        Some(thinking.to_string())
    }
}

/// Map Cursor tool names to canonical display names.
fn map_cursor_tool_name(name: &str) -> &str {
    match name {
        "Shell" | "shell" => "Bash",
        "Write" | "write" => "Write",
        "Read" | "read" => "Read",
        "StrReplace" | "str_replace" => "Edit",
        "Glob" | "glob" => "Glob",
        "Grep" | "grep" => "Grep",
        "Delete" | "delete" => "Delete",
        "ReadLints" => "Lint",
        _ => name,
    }
}

/// Remap tool args to match canonical format for frontend display.
fn remap_tool_args(tool_name: &str, args: &Value) -> Option<String> {
    let obj = args.as_object()?;
    match tool_name {
        "Bash" => {
            let cmd = obj.get("command").or_else(|| obj.get("input")).and_then(|c| c.as_str())?;
            Some(serde_json::json!({"command": cmd}).to_string())
        }
        "Write" => {
            let path = obj.get("path").or_else(|| obj.get("file_path")).and_then(|p| p.as_str())?;
            Some(serde_json::json!({"file_path": path}).to_string())
        }
        "Read" => {
            let path = obj.get("path").or_else(|| obj.get("file_path")).and_then(|p| p.as_str())?;
            Some(serde_json::json!({"file_path": path}).to_string())
        }
        "Edit" => {
            let path = obj.get("path").or_else(|| obj.get("file_path")).and_then(|p| p.as_str()).unwrap_or("");
            let old = obj.get("old_str").or_else(|| obj.get("old_string")).and_then(|s| s.as_str()).unwrap_or("");
            let new = obj.get("new_str").or_else(|| obj.get("new_string")).and_then(|s| s.as_str()).unwrap_or("");
            Some(serde_json::json!({"file_path": path, "old_string": old, "new_string": new}).to_string())
        }
        "Glob" => {
            let pattern = obj.get("pattern").and_then(|p| p.as_str())?;
            Some(serde_json::json!({"pattern": pattern}).to_string())
        }
        "Grep" => {
            let pattern = obj.get("pattern").and_then(|p| p.as_str())?;
            let path = obj.get("path").and_then(|p| p.as_str());
            let mut j = serde_json::json!({"pattern": pattern});
            if let Some(p) = path {
                j["path"] = serde_json::json!(p);
            }
            Some(j.to_string())
        }
        _ => Some(args.to_string()),
    }
}

impl SessionProvider for CursorProvider {
    fn provider(&self) -> Provider {
        Provider::Cursor
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        let chats = self.chats_dir();
        if chats.exists() {
            vec![chats]
        } else {
            Vec::new()
        }
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let chats_dir = self.chats_dir();
        if !chats_dir.exists() {
            return Ok(Vec::new());
        }

        let db_files: Vec<PathBuf> = WalkDir::new(&chats_dir)
            .max_depth(4)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() == "store.db")
            .map(|e| e.into_path())
            .collect();

        let sessions: Vec<ParsedSession> = db_files
            .iter()
            .filter_map(|path| self.parse_session_db(path))
            .collect();

        Ok(sessions)
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let db_path = Path::new(source_path);
        let conn = Self::open_db(db_path).ok_or_else(|| {
            ProviderError::Parse("failed to open Cursor DB".to_string())
        })?;

        let rows = Self::read_blobs(&conn);
        let mut messages = Vec::new();
        // Map toolCallId -> message index for merging tool results
        let mut call_id_map: HashMap<String, usize> = HashMap::new();

        for raw in &rows {
            let msg: Value = match serde_json::from_str(raw) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

            match role {
                "system" => continue,
                "user" => {
                    let text = extract_text_content(&msg);
                    let clean = extract_user_text(&text);
                    if clean.is_empty() {
                        continue;
                    }

                    messages.push(Message {
                        role: MessageRole::User,
                        content: clean,
                        timestamp: None,
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                    });
                }
                "assistant" => {
                    let text = extract_text_content(&msg);

                    // Extract thinking
                    if let Some(thinking) = extract_think_content(&text) {
                        messages.push(Message {
                            role: MessageRole::System,
                            content: format!("[thinking]\n{thinking}"),
                            timestamp: None,
                            tool_name: None,
                            tool_input: None,
                            token_usage: None,
                        });
                    }

                    // Emit visible text (without think tags)
                    let visible = strip_think_tags(&text);
                    if !visible.is_empty() {
                        messages.push(Message {
                            role: MessageRole::Assistant,
                            content: visible,
                            timestamp: None,
                            tool_name: None,
                            tool_input: None,
                            token_usage: None,
                        });
                    }

                    // Extract tool calls from assistant content
                    let content_arr = parse_content_array(msg.get("content"));
                    for part in &content_arr {
                        if part.get("type").and_then(|t| t.as_str()) != Some("tool-call") {
                            continue;
                        }
                        let raw_name = part.get("toolName").and_then(|n| n.as_str()).unwrap_or("tool");
                        let display_name = map_cursor_tool_name(raw_name);
                        let args = part.get("args");
                        let tool_input = args.and_then(|a| remap_tool_args(display_name, a));

                        // Track call_id for result merging
                        let idx = messages.len();
                        if let Some(call_id) = part.get("toolCallId").and_then(|id| id.as_str()) {
                            call_id_map.insert(call_id.to_string(), idx);
                        }

                        messages.push(Message {
                            role: MessageRole::Tool,
                            content: String::new(),
                            timestamp: None,
                            tool_name: Some(display_name.to_string()),
                            tool_input,
                            token_usage: None,
                        });
                    }
                }
                "tool" => {
                    let content_arr = parse_content_array(msg.get("content"));
                    for part in &content_arr {
                        if part.get("type").and_then(|t| t.as_str()) != Some("tool-result") {
                            continue;
                        }
                        let result = part.get("result").and_then(|r| r.as_str()).unwrap_or("").to_string();
                        let call_id = part.get("toolCallId").and_then(|id| id.as_str());

                        // Merge into matching tool-call by callId
                        if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied() {
                            if idx < messages.len() {
                                messages[idx].content = result;
                                continue;
                            }
                        }

                        // Fallback: standalone tool result
                        if !result.is_empty() {
                            let tool_name = part.get("toolName").and_then(|n| n.as_str()).unwrap_or("tool");
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: result,
                                timestamp: None,
                                tool_name: Some(map_cursor_tool_name(tool_name).to_string()),
                                tool_input: None,
                                token_usage: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(messages)
    }
}
