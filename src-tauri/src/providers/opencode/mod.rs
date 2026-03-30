mod parser;

use parser::{capitalize_tool, extract_tokens, ms_to_rfc3339};

use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};

pub struct OpenCodeProvider {
    db_path: PathBuf,
}

impl OpenCodeProvider {
    pub fn new() -> Option<Self> {
        // OpenCode stores its DB in XDG_DATA_HOME/opencode/ (~/.local/share/opencode/ on macOS/Linux)
        let base = if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(xdg)
        } else {
            dirs::home_dir()?.join(".local").join("share")
        };
        let data_dir = base.join("opencode");
        Some(Self {
            db_path: data_dir.join("opencode.db"),
        })
    }

    fn open_db(&self) -> Result<Connection, ProviderError> {
        if !self.db_path.exists() {
            return Err(ProviderError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("OpenCode database not found: {}", self.db_path.display()),
            )));
        }
        let conn = Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        // Ensure WAL reads see latest committed data
        let _ = conn.pragma_update(None, "journal_mode", "wal");
        // Prevent accidental writes to external database
        let _ = conn.pragma_update(None, "query_only", "ON");
        Ok(conn)
    }
}

impl SessionProvider for OpenCodeProvider {
    fn provider(&self) -> Provider {
        Provider::OpenCode
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        if self.db_path.exists() {
            vec![self.db_path.parent().unwrap_or(&self.db_path).to_path_buf()]
        } else {
            Vec::new()
        }
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let conn = match self.open_db() {
            Ok(c) => c,
            Err(_) => return Ok(Vec::new()),
        };

        // Batch: message counts per session (avoids N+1)
        let mut msg_count_map: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        {
            let mut stmt =
                conn.prepare("SELECT session_id, COUNT(*) FROM message GROUP BY session_id")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for r in rows.flatten() {
                msg_count_map.insert(r.0, r.1 as u32);
            }
        }

        // Batch: content text per session from text parts (avoids N+1)
        // We collect up to 50 text parts per session using a window function.
        let mut content_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        {
            let mut stmt = conn.prepare(
                "SELECT session_id, json_extract(data, '$.text') FROM part
                 WHERE json_extract(data, '$.type') = 'text'
                 ORDER BY session_id, time_created",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?;
            let mut counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for r in rows.flatten() {
                let (sid, text) = r;
                let count = counts.entry(sid.clone()).or_insert(0);
                if *count >= 50 {
                    continue;
                }
                *count += 1;
                if let Some(t) = text {
                    content_map
                        .entry(sid)
                        .or_default()
                        .push_str(&format!("{}\n", t));
                }
            }
        }

        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.directory, s.time_created, s.time_updated,
                    s.parent_id, p.worktree, p.name
             FROM session s
             LEFT JOIN project p ON s.project_id = p.id
             ORDER BY s.time_updated DESC",
        )?;

        let sessions: Vec<ParsedSession> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,         // id
                    row.get::<_, String>(1)?,         // title
                    row.get::<_, String>(2)?,         // directory
                    row.get::<_, i64>(3)?,            // time_created
                    row.get::<_, i64>(4)?,            // time_updated
                    row.get::<_, Option<String>>(5)?, // parent_id
                    row.get::<_, Option<String>>(6)?, // worktree
                    row.get::<_, Option<String>>(7)?, // project name
                ))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(
                    id,
                    title,
                    directory,
                    time_created,
                    time_updated,
                    parent_id,
                    worktree,
                    project_name,
                )| {
                    let msg_count = msg_count_map.get(&id).copied().unwrap_or(0);
                    let content_text = content_map.get(&id).cloned().unwrap_or_default();

                    // Prefer session.directory (actual working dir);
                    // fall back to project.worktree only if directory is empty.
                    // The "global" project has worktree="/", which is not useful.
                    let project_path = if directory.is_empty() || directory == "/" {
                        worktree
                            .filter(|w| w != "/")
                            .unwrap_or_else(|| directory.clone())
                    } else {
                        directory.clone()
                    };
                    let display_title = if title.is_empty() {
                        session_title(Some(&content_text.chars().take(200).collect::<String>()))
                    } else {
                        title
                    };

                    let is_sidechain = parent_id.is_some();

                    ParsedSession {
                        meta: SessionMeta {
                            id,
                            provider: Provider::OpenCode,
                            title: display_title,
                            project_path: project_path.clone(),
                            project_name: project_name.unwrap_or_else(|| {
                                std::path::Path::new(&project_path)
                                    .file_name()
                                    .map(|n| n.to_string_lossy().to_string())
                                    .unwrap_or_default()
                            }),
                            created_at: time_created / 1000,
                            updated_at: time_updated / 1000,
                            message_count: msg_count,
                            file_size_bytes: 0,
                            source_path: self.db_path.to_string_lossy().to_string(),
                            is_sidechain,
                            variant_name: None,
                        },
                        messages: Vec::new(),
                        content_text: truncate_to_bytes(&content_text, FTS_CONTENT_LIMIT),
                    }
                },
            )
            .collect();

        Ok(sessions)
    }

    fn load_messages(
        &self,
        session_id: &str,
        _source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let conn = self.open_db()?;

        // Load all messages for this session
        let mut msg_stmt = conn.prepare(
            "SELECT m.id, m.data FROM message m
             WHERE m.session_id = ?1
             ORDER BY m.time_created",
        )?;

        let msg_rows: Vec<(String, String)> = msg_stmt
            .query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Load all parts for this session, grouped by message_id
        let mut part_stmt = conn.prepare(
            "SELECT message_id, data FROM part
             WHERE session_id = ?1
             ORDER BY id",
        )?;

        let mut parts_by_msg: std::collections::HashMap<String, Vec<serde_json::Value>> =
            std::collections::HashMap::new();
        let part_rows: Vec<(String, String)> = part_stmt
            .query_map(params![session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (mid, data) in part_rows {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) {
                parts_by_msg.entry(mid).or_default().push(v);
            }
        }

        let mut messages = Vec::new();

        for (msg_id, msg_data) in &msg_rows {
            let msg_json: serde_json::Value = serde_json::from_str(msg_data).unwrap_or_default();
            let role_str = msg_json
                .get("role")
                .and_then(|r| r.as_str())
                .unwrap_or("user");

            let timestamp = msg_json
                .get("time")
                .and_then(|t| t.get("created"))
                .and_then(|c| c.as_i64())
                .and_then(ms_to_rfc3339);

            let parts = parts_by_msg.get(msg_id).cloned().unwrap_or_default();

            match role_str {
                "user" => {
                    // Collect text parts as user message content
                    let text_content: Vec<&str> = parts
                        .iter()
                        .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"))
                        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                        .collect();

                    if !text_content.is_empty() {
                        messages.push(Message {
                            role: MessageRole::User,
                            content: text_content.join("\n"),
                            timestamp: timestamp.clone(),
                            tool_name: None,
                            tool_input: None,
                            token_usage: None,
                        });
                    }

                    // Check for file parts (images)
                    for part in &parts {
                        if part.get("type").and_then(|t| t.as_str()) == Some("file") {
                            let mime = part.get("mime").and_then(|m| m.as_str()).unwrap_or("");
                            if mime.starts_with("image/") {
                                let url = part.get("url").and_then(|u| u.as_str()).unwrap_or("");
                                if !url.is_empty() {
                                    messages.push(Message {
                                        role: MessageRole::User,
                                        content: format!("[Image: source: {url}]"),
                                        timestamp: timestamp.clone(),
                                        tool_name: None,
                                        tool_input: None,
                                        token_usage: None,
                                    });
                                }
                            }
                        }
                    }
                }
                "assistant" => {
                    let token_usage = extract_tokens(&msg_json);

                    // Collect text parts
                    let mut text_parts: Vec<String> = Vec::new();
                    // Collect tool parts to emit after the text message
                    let mut tool_messages: Vec<Message> = Vec::new();

                    for part in &parts {
                        let part_type = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match part_type {
                            "text" => {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    if !text.is_empty() {
                                        text_parts.push(text.to_string());
                                    }
                                }
                            }
                            "reasoning" => {
                                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                    if !text.trim().is_empty() {
                                        let reasoning_ts = part
                                            .get("time")
                                            .and_then(|t| t.get("start"))
                                            .and_then(|s| s.as_i64())
                                            .and_then(ms_to_rfc3339)
                                            .or_else(|| timestamp.clone());
                                        messages.push(Message {
                                            role: MessageRole::System,
                                            content: format!("[thinking]\n{text}"),
                                            timestamp: reasoning_ts,
                                            tool_name: None,
                                            tool_input: None,
                                            token_usage: None,
                                        });
                                    }
                                }
                            }
                            "tool" => {
                                let tool_name = part
                                    .get("tool")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("tool")
                                    .to_string();
                                let state = part.get("state");
                                let status = state
                                    .and_then(|s| s.get("status"))
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("");

                                // Tool input
                                let tool_input =
                                    state.and_then(|s| s.get("input")).map(|i| i.to_string());

                                // Tool output
                                let output = match status {
                                    "completed" => state
                                        .and_then(|s| s.get("output"))
                                        .and_then(|o| o.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    "error" => state
                                        .and_then(|s| s.get("error"))
                                        .and_then(|e| e.as_str())
                                        .map(|e| format!("[Error] {e}"))
                                        .unwrap_or_default(),
                                    _ => String::new(),
                                };

                                let tool_ts = state
                                    .and_then(|s| s.get("time"))
                                    .and_then(|t| t.get("start"))
                                    .and_then(|s| s.as_i64())
                                    .and_then(ms_to_rfc3339)
                                    .or_else(|| timestamp.clone());

                                // Emit tool use message
                                tool_messages.push(Message {
                                    role: MessageRole::Tool,
                                    content: output,
                                    timestamp: tool_ts,
                                    tool_name: Some(capitalize_tool(&tool_name)),
                                    tool_input,
                                    token_usage: None,
                                });
                            }
                            // Skip step-start, step-finish, reasoning, snapshot, patch, etc.
                            _ => {}
                        }
                    }

                    // Emit text message first (with token usage on last text msg of this turn)
                    if !text_parts.is_empty() {
                        messages.push(Message {
                            role: MessageRole::Assistant,
                            content: text_parts.join("\n"),
                            timestamp: timestamp.clone(),
                            tool_name: None,
                            tool_input: None,
                            token_usage: if tool_messages.is_empty() {
                                token_usage.clone()
                            } else {
                                None
                            },
                        });
                    }

                    // Emit tool messages
                    if !tool_messages.is_empty() {
                        let last_idx = tool_messages.len() - 1;
                        for (i, mut tool_msg) in tool_messages.into_iter().enumerate() {
                            // Attach token usage to last tool message if no text parts,
                            // otherwise it was already attached to the text message above
                            if i == last_idx && text_parts.is_empty() {
                                tool_msg.token_usage = token_usage.clone();
                            }
                            messages.push(tool_msg);
                        }
                    }

                    // If assistant message had no text and no tools (rare), still emit for token tracking
                    if text_parts.is_empty()
                        && !parts
                            .iter()
                            .any(|p| p.get("type").and_then(|t| t.as_str()) == Some("tool"))
                        && token_usage.is_some()
                    {
                        messages.push(Message {
                            role: MessageRole::Assistant,
                            content: String::new(),
                            timestamp,
                            tool_name: None,
                            tool_input: None,
                            token_usage,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(messages)
    }
}
