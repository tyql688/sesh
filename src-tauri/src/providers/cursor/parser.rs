use std::path::Path;

use serde_json::Value;

use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    project_name_from_path, session_title, truncate_to_bytes, FTS_CONTENT_LIMIT,
};
use crate::tool_metadata::{build_tool_metadata, ToolCallFacts};

use super::tools::*;
use super::CursorProvider;

/// Shared JSONL entry parsing used by both scan and load paths.
/// Ensures JSON parsing and role/content extraction are identical.
///
/// Returns the count of malformed lines that were logged and skipped; callers
/// thread this into `ParsedSession.parse_warning_count` / `LoadedSession.parse_warning_count`
/// so the UI can surface a `⚠ N` badge.
pub(crate) fn for_each_transcript_entry(
    content: &str,
    source_label: &str,
    mut handler: impl FnMut(&str, Option<&Value>),
) -> u32 {
    let mut parse_warning_count: u32 = 0;
    for line in content.lines() {
        let entry: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(error) => {
                log::warn!(
                    "skipping malformed Cursor transcript JSONL in '{}': {}",
                    source_label,
                    error
                );
                parse_warning_count = parse_warning_count.saturating_add(1);
                continue;
            }
        };
        let role = entry.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let msg_content = entry.get("message").and_then(|m| m.get("content"));
        handler(role, msg_content);
    }
    parse_warning_count
}

pub(crate) fn parse_transcript_messages(content: &str, source_label: &str) -> (Vec<Message>, u32) {
    let mut messages = Vec::new();

    let parse_warning_count =
        for_each_transcript_entry(content, source_label, |role, msg_content| match role {
            "user" => {
                let text = extract_text_from_content(msg_content);
                let clean = extract_user_text(&text);
                if !clean.is_empty() {
                    messages.push(Message {
                        role: MessageRole::User,
                        content: clean,
                        timestamp: None,
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                        model: None,
                        usage_hash: None,
                        tool_metadata: None,
                    });
                }
            }
            "assistant" => {
                let raw_text = extract_text_from_content(msg_content);
                let text = strip_redacted(&raw_text);

                if let Some(thinking) = extract_think_content(&text) {
                    messages.push(Message {
                        role: MessageRole::System,
                        content: format!("[thinking]\n{thinking}"),
                        timestamp: None,
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                        model: None,
                        usage_hash: None,
                        tool_metadata: None,
                    });
                }

                let after_think = strip_think_tags(&text);

                if let Some(cursor_thinking) = extract_cursor_thinking(&after_think) {
                    messages.push(Message {
                        role: MessageRole::System,
                        content: format!("[thinking]\n{cursor_thinking}"),
                        timestamp: None,
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                        model: None,
                        usage_hash: None,
                        tool_metadata: None,
                    });
                }

                let visible = strip_cursor_thinking(&after_think);
                if !visible.is_empty() {
                    messages.push(Message {
                        role: MessageRole::Assistant,
                        content: visible,
                        timestamp: None,
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                        model: None,
                        usage_hash: None,
                        tool_metadata: None,
                    });
                }

                let parts = parse_content_array(msg_content);
                for part in &parts {
                    if part.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                        continue;
                    }
                    let raw_name = part.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                    let args = part.get("input");
                    let metadata = build_tool_metadata(ToolCallFacts {
                        provider: Provider::Cursor,
                        raw_name,
                        input: args,
                        call_id: part
                            .get("id")
                            .or_else(|| part.get("tool_use_id"))
                            .and_then(|v| v.as_str()),
                        assistant_id: None,
                    });
                    let display_name = metadata.canonical_name.clone();
                    let tool_input = args.and_then(|a| remap_tool_args(&display_name, a));

                    messages.push(Message {
                        role: MessageRole::Tool,
                        content: String::new(),
                        timestamp: None,
                        tool_name: Some(display_name),
                        tool_input,
                        token_usage: None,
                        model: None,
                        usage_hash: None,
                        tool_metadata: Some(metadata),
                    });
                }
            }
            _ => {}
        });

    (messages, parse_warning_count)
}

fn extract_project_path_from_transcript(content: &str) -> String {
    let mut project_path = String::new();
    // Discard the parse-warning count here — this helper only extracts the
    // project path and the count is owned by the primary parse call.
    let _ = for_each_transcript_entry(content, "<cursor transcript>", |role, msg_content| {
        if role != "user" || !project_path.is_empty() {
            return;
        }
        let text = extract_text_from_content(msg_content);
        if let Some(wp) = extract_workspace_path(&text) {
            project_path = wp;
        }
    });
    project_path
}

impl CursorProvider {
    /// Parse a transcript JSONL file into a ParsedSession.
    /// Handles both main transcripts and subagent transcripts based on path structure.
    ///
    /// Main transcript: `.../agent-transcripts/<sessionId>/<sessionId>.jsonl`
    ///   → parent_id = None, is_sidechain = false
    ///
    /// Subagent transcript: `.../agent-transcripts/<parentId>/subagents/<subagentId>.jsonl`
    ///   → parent_id = Some(parentId), is_sidechain = true
    pub fn parse_transcript_jsonl(&self, path: &Path) -> Option<ParsedSession> {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) => {
                log::warn!(
                    "failed to read Cursor transcript '{}': {}",
                    path.display(),
                    error
                );
                return None;
            }
        };
        let (messages, parse_warning_count) =
            parse_transcript_messages(&content, &path.display().to_string());
        if messages.is_empty() {
            return None;
        }

        let file_id = path.file_stem()?.to_string_lossy().to_string();

        // Detect main vs subagent from path structure
        let is_subagent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            == Some("subagents");

        let (session_id, parent_id, is_sidechain) = if is_subagent {
            let parent_id = path
                .parent()? // subagents/
                .parent()? // <parentId>/
                .file_name()?
                .to_string_lossy()
                .to_string();
            (file_id, Some(parent_id), true)
        } else {
            (file_id, None, false)
        };

        let first_user_message = messages.iter().find_map(|message| {
            (message.role == MessageRole::User && !message.content.is_empty())
                .then(|| message.content.clone())
        });
        let content_parts: Vec<String> = messages
            .iter()
            .filter(|message| {
                matches!(message.role, MessageRole::User | MessageRole::Assistant)
                    && !message.content.is_empty()
            })
            .map(|message| message.content.clone())
            .collect();
        let mut project_path = extract_project_path_from_transcript(&content);
        let message_count = messages.len() as u32;

        // For subagents without Workspace Path in content, derive from projects dir name
        if project_path.is_empty() && !is_subagent {
            project_path = self.derive_project_path_from_transcript(path);
        }
        if project_path.is_empty() && is_subagent {
            // Try deriving from the parent transcript's project dir
            if let Some(parent_dir) = path.parent().and_then(|p| p.parent()) {
                let main_jsonl_dir = parent_dir;
                project_path =
                    self.derive_project_path_from_transcript(&main_jsonl_dir.join("dummy.jsonl"));
            }
        }

        let project_name = project_name_from_path(&project_path);

        // For subagents, try to use the description from the parent's Subagent tool_use.
        // Also get the match index so we can order children by their Task position.
        let (title, task_index) = if is_subagent {
            match self.find_subagent_description(path, first_user_message.as_deref().unwrap_or(""))
            {
                Some((desc, idx)) => (desc, idx as i64),
                None => (session_title(first_user_message.as_deref()), 0),
            }
        } else {
            (session_title(first_user_message.as_deref()), 0)
        };

        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                log::warn!(
                    "failed to read Cursor transcript metadata '{}': {}",
                    path.display(),
                    error
                );
                return None;
            }
        };
        let file_size = metadata.len();

        let created_at = match metadata.created() {
            Ok(time) => match time.duration_since(std::time::UNIX_EPOCH) {
                Ok(duration) => duration.as_secs() as i64 + task_index,
                Err(error) => {
                    log::warn!(
                        "failed to convert Cursor transcript created time '{}' to unix timestamp: {}",
                        path.display(),
                        error
                    );
                    return None;
                }
            },
            Err(error) => {
                log::warn!(
                    "failed to read Cursor transcript created time '{}': {}",
                    path.display(),
                    error
                );
                return None;
            }
        };
        let updated_at = match metadata.modified() {
            Ok(time) => match time.duration_since(std::time::UNIX_EPOCH) {
                Ok(duration) => duration.as_secs() as i64,
                Err(error) => {
                    log::warn!(
                        "failed to convert Cursor transcript modified time '{}' to unix timestamp: {}",
                        path.display(),
                        error
                    );
                    return None;
                }
            },
            Err(error) => {
                log::warn!(
                    "failed to read Cursor transcript modified time '{}': {}",
                    path.display(),
                    error
                );
                return None;
            }
        };

        Some(ParsedSession {
            meta: SessionMeta {
                id: session_id,
                provider: Provider::Cursor,
                title,
                project_path,
                project_name,
                created_at,
                updated_at,
                message_count,
                file_size_bytes: file_size,
                source_path: path.to_string_lossy().to_string(),
                is_sidechain,
                variant_name: None,
                model: None,
                cc_version: None,
                git_branch: None,
                parent_id,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        },
            messages: Vec::new(),
            content_text: truncate_to_bytes(&content_parts.join("\n"), FTS_CONTENT_LIMIT),
            parse_warning_count,
        })
    }

    /// Find the `description` from the parent transcript's Subagent/Task tool_use
    /// that matches this subagent's first user message (which is the `prompt`).
    /// Returns (description, match_index) where index preserves Task order.
    fn find_subagent_description(
        &self,
        subagent_path: &Path,
        first_user_msg: &str,
    ) -> Option<(String, usize)> {
        let parent_dir = subagent_path.parent()?.parent()?; // subagents/ → <parentId>/
        let parent_id = parent_dir.file_name()?.to_string_lossy();
        let parent_jsonl = parent_dir.join(format!("{parent_id}.jsonl"));

        let content = match std::fs::read_to_string(&parent_jsonl) {
            Ok(content) => content,
            Err(error) => {
                log::warn!(
                    "failed to read Cursor parent transcript '{}': {}",
                    parent_jsonl.display(),
                    error
                );
                return None;
            }
        };

        // Collect all Task/Subagent tool_uses with their prompts and descriptions
        let mut candidates: Vec<(String, String)> = Vec::new();
        for line in content.lines() {
            let entry: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(error) => {
                    log::warn!(
                        "skipping malformed Cursor parent transcript JSONL in '{}': {}",
                        parent_jsonl.display(),
                        error
                    );
                    continue;
                }
            };
            if entry.get("role").and_then(|r| r.as_str()) != Some("assistant") {
                continue;
            }

            let msg_content = entry.get("message").and_then(|m| m.get("content"));
            let parts = parse_content_array(msg_content);

            for part in &parts {
                if part.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
                    continue;
                }
                let name = part.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if name != "Subagent" && name != "Task" {
                    continue;
                }

                let input = match part.get("input") {
                    Some(i) => i,
                    None => continue,
                };
                let description = input
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                let prompt = input.get("prompt").and_then(|p| p.as_str()).unwrap_or("");
                if !description.is_empty() && !prompt.is_empty() {
                    candidates.push((prompt.to_string(), description.to_string()));
                }
            }
        }

        // Match subagent's first user message against candidates.
        // Use full text comparison to avoid ambiguity with shared prefixes.
        for (idx, (prompt, desc)) in candidates.iter().enumerate() {
            if prompt == first_user_msg
                || first_user_msg.starts_with(prompt.as_str())
                || prompt.starts_with(first_user_msg)
            {
                return Some((desc.clone(), idx));
            }
        }
        None
    }

    /// Derive workspace path from the projects directory name.
    /// Path structure: ~/.cursor/projects/<ProjectKey>/agent-transcripts/<id>/<id>.jsonl
    /// ProjectKey format: Users-john-Documents-myproject (dashes replace path separators)
    ///
    /// We try to find a real directory matching the decoded path.
    fn derive_project_path_from_transcript(&self, path: &Path) -> String {
        // Walk up from the transcript file to find the project dir
        let mut current = path.parent(); // <sessionId>/
        while let Some(dir) = current {
            if dir.file_name().and_then(|n| n.to_str()) == Some("agent-transcripts") {
                // Parent of agent-transcripts is the project key dir
                if let Some(project_key_dir) = dir.parent() {
                    let key = project_key_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    return decode_project_key(key);
                }
            }
            current = dir.parent();
        }
        String::new()
    }
}

/// Decode a Cursor projects directory name back to a filesystem path.
/// e.g. "Users-john-Documents-myproject" → "/Users/john/Documents/myproject"
///
/// Strategy: greedily reconstruct the longest valid path by trying dash positions
/// as potential path separators.
fn decode_project_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    // Simple heuristic: replace dashes with "/" and prepend "/"
    let candidate = format!("/{}", key.replace('-', "/"));
    if Path::new(&candidate).is_dir() {
        return candidate;
    }

    // Greedy reconstruction: split by dash, try to join segments into existing dirs
    let segments: Vec<&str> = key.split('-').collect();
    let mut path = String::from("/");
    let mut i = 0;
    while i < segments.len() {
        // Try longest match first: join remaining segments with dashes
        let mut best_len = 0;
        for end in (i + 1..=segments.len()).rev() {
            let part = segments[i..end].join("-");
            let test = if path == "/" {
                format!("/{part}")
            } else {
                format!("{path}/{part}")
            };
            if Path::new(&test).exists() {
                path = test;
                best_len = end - i;
                break;
            }
        }
        if best_len == 0 {
            // No existing path found; use single segment
            if path == "/" {
                path = format!("/{}", segments[i]);
            } else {
                path = format!("{}/{}", path, segments[i]);
            }
            i += 1;
        } else {
            i += best_len;
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use crate::models::MessageRole;
    use crate::providers::cursor::CursorProvider;

    fn write_fixture(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
        let session_id = "test-session-001";
        let transcript_dir = dir
            .join("projects")
            .join("TestProj")
            .join("agent-transcripts")
            .join(session_id);
        std::fs::create_dir_all(&transcript_dir).unwrap();
        let path = transcript_dir.join(format!("{session_id}.jsonl"));
        std::fs::write(&path, content).unwrap();
        // Create store.db marker so it's treated as CLI session
        let chats_dir = dir.join("chats").join("hash123").join(session_id);
        std::fs::create_dir_all(&chats_dir).unwrap();
        std::fs::File::create(chats_dir.join("store.db")).unwrap();
        path
    }

    #[test]
    fn test_scan_and_load_message_count_consistent() {
        let dir = tempfile::TempDir::new().unwrap();
        let fixture = r#"{"role":"user","message":{"content":[{"type":"text","text":"hello world"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"hi there"},{"type":"tool_use","name":"Read","id":"t1","input":{"file_path":"/tmp/f"}}]}}
{"role":"user","message":{"content":[{"type":"text","text":"thanks"}]}}"#;

        let path = write_fixture(dir.path(), fixture);
        let provider = CursorProvider {
            home_dir: dir.path().to_path_buf(),
        };

        let parsed = provider.parse_transcript_jsonl(&path).unwrap();
        let scan_count = parsed.meta.message_count;

        let messages = provider
            .load_transcript_messages(path.to_str().unwrap())
            .unwrap();
        let load_count = messages.len() as u32;

        assert_eq!(
            scan_count, load_count,
            "scan message_count ({scan_count}) != load messages.len() ({load_count})"
        );
    }

    #[test]
    fn test_scan_and_load_thinking_consistent() {
        let dir = tempfile::TempDir::new().unwrap();
        let fixture = r#"{"role":"user","message":{"content":[{"type":"text","text":"do something"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"<think>internal reasoning</think>\nvisible output"}]}}"#;

        let path = write_fixture(dir.path(), fixture);
        let provider = CursorProvider {
            home_dir: dir.path().to_path_buf(),
        };

        let messages = provider
            .load_transcript_messages(path.to_str().unwrap())
            .unwrap();
        let has_thinking = messages
            .iter()
            .any(|m| m.role == MessageRole::System && m.content.starts_with("[thinking]"));

        assert!(has_thinking, "load path should extract thinking blocks");
    }

    #[test]
    fn test_scan_and_load_message_count_consistent_with_thinking_and_redaction() {
        let dir = tempfile::TempDir::new().unwrap();
        let fixture = r#"{"role":"user","message":{"content":[{"type":"text","text":"do something"}]}}
{"role":"assistant","message":{"content":[{"type":"text","text":"<think>internal reasoning</think>\nvisible output\n\n**Plan**\nuse ripgrep first\n[REDACTED]"},{"type":"tool_use","name":"Read","id":"t1","input":{"path":"/tmp/file"}}]}}"#;

        let path = write_fixture(dir.path(), fixture);
        let provider = CursorProvider {
            home_dir: dir.path().to_path_buf(),
        };

        let parsed = provider.parse_transcript_jsonl(&path).unwrap();
        let messages = provider
            .load_transcript_messages(path.to_str().unwrap())
            .unwrap();

        assert_eq!(parsed.meta.message_count, messages.len() as u32);
        assert_eq!(parsed.content_text, "do something\nvisible output");
        assert!(!parsed.content_text.contains("internal reasoning"));
        assert!(!parsed.content_text.contains("[REDACTED]"));
    }
}
