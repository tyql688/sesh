mod parser;
mod tools;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use rusqlite::Connection;
use serde_json::Value;
use walkdir::WalkDir;

use crate::models::{Message, MessageRole, Provider};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};

use tools::*;

pub struct CursorProvider {
    home_dir: PathBuf,
}

impl CursorProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    fn chats_dir(&self) -> PathBuf {
        self.home_dir.join(".cursor").join("chats")
    }

    pub(crate) fn open_db(db_path: &Path) -> Option<Connection> {
        // Use READ_WRITE to allow reading uncommitted WAL data.
        // SQLITE_OPEN_READ_ONLY cannot reliably read WAL in shared-cache mode.
        let conn = Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        // Ensure WAL reads see latest committed data
        let _ = conn.pragma_update(None, "journal_mode", "wal");
        // Prevent accidental writes to external database
        let _ = conn.pragma_update(None, "query_only", "ON");
        Some(conn)
    }

    /// Read all blobs as UTF-8 strings, ordered by rowid.
    pub(crate) fn read_blobs(conn: &Connection) -> Vec<String> {
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
            .par_iter()
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
        let conn = Self::open_db(db_path)
            .ok_or_else(|| ProviderError::Parse("failed to open Cursor DB".to_string()))?;

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
                        model: None,
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
                            model: None,
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
                            model: None,
                        });
                    }

                    // Extract tool calls from assistant content
                    let content_arr = parse_content_array(msg.get("content"));
                    for part in &content_arr {
                        if part.get("type").and_then(|t| t.as_str()) != Some("tool-call") {
                            continue;
                        }
                        let raw_name = part
                            .get("toolName")
                            .and_then(|n| n.as_str())
                            .unwrap_or("tool");
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
                            model: None,
                        });
                    }
                }
                "tool" => {
                    let content_arr = parse_content_array(msg.get("content"));
                    for part in &content_arr {
                        if part.get("type").and_then(|t| t.as_str()) != Some("tool-result") {
                            continue;
                        }
                        let result = part
                            .get("result")
                            .and_then(|r| r.as_str())
                            .unwrap_or("")
                            .to_string();
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
                            let tool_name = part
                                .get("toolName")
                                .and_then(|n| n.as_str())
                                .unwrap_or("tool");
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: result,
                                timestamp: None,
                                tool_name: Some(map_cursor_tool_name(tool_name).to_string()),
                                tool_input: None,
                                token_usage: None,
                                model: None,
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
