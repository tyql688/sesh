use std::path::Path;

use serde_json::Value;

use crate::models::{Provider, SessionMeta};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    project_name_from_path, session_title, truncate_to_bytes, FTS_CONTENT_LIMIT,
};

use super::tools::*;
use super::CursorProvider;

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
        let content = std::fs::read_to_string(path).ok()?;

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

        let mut first_user_message: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut message_count: u32 = 0;
        let mut project_path = String::new();

        for line in content.lines() {
            let entry: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let role = entry.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let msg_content = entry.get("message").and_then(|m| m.get("content"));

            match role {
                "user" => {
                    let text = extract_text_from_content(msg_content);
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
                    let text = extract_text_from_content(msg_content);
                    let after_think = strip_think_tags(&text);
                    let clean = strip_cursor_thinking(&after_think);
                    if !clean.is_empty() {
                        content_parts.push(clean);
                    }
                    let parts = parse_content_array(msg_content);
                    let tool_count = parts
                        .iter()
                        .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("tool_use"))
                        .count();
                    message_count += 1 + tool_count as u32;
                }
                _ => {}
            }
        }

        if message_count == 0 {
            return None;
        }

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

        let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        let created_at = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.created().ok())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .unwrap_or(0)
            + task_index; // tiebreaker: preserve parent Task order
        let updated_at = std::fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .unwrap_or(created_at);

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
            },
            messages: Vec::new(),
            content_text: truncate_to_bytes(&content_parts.join("\n"), FTS_CONTENT_LIMIT),
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

        let content = std::fs::read_to_string(&parent_jsonl).ok()?;

        // Collect all Task/Subagent tool_uses with their prompts and descriptions
        let mut candidates: Vec<(String, String)> = Vec::new();
        for line in content.lines() {
            let entry: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
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
    /// ProjectKey format: Users-john-Documents-drama-aitool (dashes replace path separators)
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
/// e.g. "Users-john-Documents-drama-aitool" → "/Users/john/Documents/drama/aitool"
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
