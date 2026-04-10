use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde_json::Value;

use crate::models::{Message, MessageRole, Provider, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    parse_rfc3339_timestamp, project_name_from_path, session_title, truncate_to_bytes,
    FTS_CONTENT_LIMIT,
};

/// Map Copilot tool names to canonical CC Session names.
fn canonical_tool_name(name: &str) -> &str {
    match name {
        "bash" => "Bash",
        "view" => "Read",
        "create" => "Write",
        "edit" => "Edit",
        "glob" => "Glob",
        "grep" => "Grep",
        "task" | "read_agent" => "Agent",
        "ask_user" => "AskUser",
        "sql" => "SQL",
        // Internal tools — skip in display but keep for merge
        "report_intent" | "task_complete" => name,
        other => other,
    }
}

/// Returns true for internal tool names that should not produce visible messages.
fn is_internal_tool(name: &str) -> bool {
    matches!(name, "report_intent" | "task_complete")
}

/// Extract user-visible content from `user.message`.
/// Replaces Copilot's `[📷 displayName]` placeholders with `[Image: source: path]`
/// using the full path from attachments.
fn extract_user_content(data: &Value) -> String {
    let mut content = data
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    if let Some(attachments) = data.get("attachments").and_then(|a| a.as_array()) {
        for att in attachments {
            if att.get("type").and_then(|t| t.as_str()) == Some("file") {
                if let Some(display) = att.get("displayName").and_then(|d| d.as_str()) {
                    if is_image_filename(display) {
                        let path = att.get("path").and_then(|p| p.as_str()).unwrap_or(display);
                        let placeholder = format!("[📷 {display}]");
                        let replacement = format!("[Image: source: {path}]");
                        content = content.replace(&placeholder, &replacement);
                    }
                }
            }
        }
    }

    content
}

fn is_image_filename(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".svg")
}

pub fn parse_session_file(path: &PathBuf) -> Option<ParsedSession> {
    let file = File::open(path).ok()?;
    let metadata = fs::metadata(path).ok()?;
    let file_size = metadata.len();

    let reader = BufReader::new(file);
    let mut messages: Vec<Message> = Vec::new();
    let mut content_parts: Vec<String> = Vec::new();
    let mut first_user_message: Option<String> = None;
    let mut first_timestamp: Option<String> = None;
    let mut last_timestamp: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut cc_version: Option<String> = None;
    let mut git_branch: Option<String> = None;
    // Map toolCallId → index in messages vec for merging tool results
    let mut call_id_map: HashMap<String, usize> = HashMap::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        if line.trim().is_empty() {
            continue;
        }

        let entry: Value = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("skipping malformed JSONL in '{}': {}", path.display(), e);
                continue;
            }
        };

        let event_type = match entry.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        // Track timestamps
        if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
            if first_timestamp.is_none() {
                first_timestamp = Some(ts.to_string());
            }
            last_timestamp = Some(ts.to_string());
        }

        let timestamp = entry
            .get("timestamp")
            .and_then(|t| t.as_str())
            .map(std::string::ToString::to_string);

        let data = match entry.get("data") {
            Some(d) => d,
            None => continue,
        };

        match event_type.as_str() {
            "session.start" => {
                if cwd.is_none() {
                    if let Some(c) = data
                        .get("context")
                        .and_then(|ctx| ctx.get("cwd"))
                        .and_then(|c| c.as_str())
                    {
                        if !c.is_empty() {
                            cwd = Some(c.to_string());
                        }
                    }
                }
                if git_branch.is_none() {
                    if let Some(b) = data
                        .get("context")
                        .and_then(|ctx| ctx.get("branch"))
                        .and_then(|b| b.as_str())
                    {
                        if !b.is_empty() && b != "HEAD" {
                            git_branch = Some(b.to_string());
                        }
                    }
                }
                if cc_version.is_none() {
                    if let Some(v) = data.get("copilotVersion").and_then(|v| v.as_str()) {
                        if !v.is_empty() {
                            cc_version = Some(v.to_string());
                        }
                    }
                }
            }
            "user.message" => {
                let content = extract_user_content(data);

                if first_user_message.is_none() && !content.is_empty() {
                    first_user_message = Some(content.clone());
                }

                if !content.is_empty() {
                    content_parts.push(content.clone());
                    messages.push(Message {
                        role: MessageRole::User,
                        content,
                        timestamp,
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                        model: None,
                        usage_hash: None,
                    });
                }
            }
            "assistant.message" => {
                // Thinking from reasoningText
                if let Some(reasoning) = data.get("reasoningText").and_then(|r| r.as_str()) {
                    if !reasoning.is_empty() {
                        messages.push(Message {
                            role: MessageRole::System,
                            content: format!("[thinking]\n{reasoning}"),
                            timestamp: timestamp.clone(),
                            tool_name: None,
                            tool_input: None,
                            token_usage: None,
                            model: None,
                            usage_hash: None,
                        });
                    }
                }

                // Assistant text content
                let text = data
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();

                // Token usage
                let token_usage = data
                    .get("outputTokens")
                    .and_then(|o| o.as_u64())
                    .filter(|&t| t > 0)
                    .map(|output| TokenUsage {
                        input_tokens: 0,
                        output_tokens: output as u32,
                        cache_creation_input_tokens: 0,
                        cache_read_input_tokens: 0,
                    });

                let has_text = !text.is_empty();
                if has_text {
                    content_parts.push(text.clone());
                    messages.push(Message {
                        role: MessageRole::Assistant,
                        content: text,
                        timestamp: timestamp.clone(),
                        tool_name: None,
                        tool_input: None,
                        token_usage: token_usage.clone(),
                        model: None,
                        usage_hash: None,
                    });
                }

                // Tool requests embedded in assistant message
                let mut usage_attached = has_text; // already on assistant msg
                if let Some(tool_requests) = data.get("toolRequests").and_then(|t| t.as_array()) {
                    for tr in tool_requests {
                        let name = tr.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        if is_internal_tool(name) {
                            continue;
                        }
                        let call_id = tr
                            .get("toolCallId")
                            .and_then(|id| id.as_str())
                            .unwrap_or("")
                            .to_string();
                        let canonical = canonical_tool_name(name);
                        let args = tr
                            .get("arguments")
                            .map(|a| serde_json::to_string(a).unwrap_or_default())
                            .unwrap_or_default();

                        // Attach token usage to the first tool msg when assistant
                        // content was empty (pure tool-call turns).
                        let tool_usage = if !usage_attached {
                            usage_attached = true;
                            token_usage.clone()
                        } else {
                            None
                        };

                        let idx = messages.len();
                        messages.push(Message {
                            role: MessageRole::Tool,
                            content: String::new(),
                            timestamp: timestamp.clone(),
                            tool_name: Some(canonical.to_string()),
                            tool_input: Some(args),
                            token_usage: tool_usage,
                            model: None,
                            usage_hash: None,
                        });
                        if !call_id.is_empty() {
                            call_id_map.insert(call_id, idx);
                        }
                    }
                }
            }
            "tool.execution_start" => {
                let tool_name = data.get("toolName").and_then(|n| n.as_str()).unwrap_or("");
                if is_internal_tool(tool_name) {
                    continue;
                }
                let call_id = data
                    .get("toolCallId")
                    .and_then(|id| id.as_str())
                    .unwrap_or("")
                    .to_string();

                // Only create a new tool message if we don't already have one
                // from the assistant.message toolRequests
                if !call_id.is_empty() && call_id_map.contains_key(&call_id) {
                    // Already tracked from toolRequests — update args if richer
                    if let Some(&idx) = call_id_map.get(&call_id) {
                        if let Some(msg) = messages.get_mut(idx) {
                            // If the execution_start has arguments and current tool_input is empty,
                            // use the execution_start arguments
                            if msg
                                .tool_input
                                .as_deref()
                                .is_none_or(|s| s.is_empty() || s == "{}")
                            {
                                if let Some(args) = data.get("arguments") {
                                    let args_str = serde_json::to_string(args).unwrap_or_default();
                                    if !args_str.is_empty() && args_str != "{}" {
                                        msg.tool_input = Some(args_str);
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }

                // Subagent tool call (parentToolCallId present) — create new entry
                let canonical = canonical_tool_name(tool_name);
                let args = data
                    .get("arguments")
                    .map(|a| serde_json::to_string(a).unwrap_or_default())
                    .unwrap_or_default();

                let idx = messages.len();
                messages.push(Message {
                    role: MessageRole::Tool,
                    content: String::new(),
                    timestamp: timestamp.clone(),
                    tool_name: Some(canonical.to_string()),
                    tool_input: Some(args),
                    token_usage: None,
                    model: None,
                    usage_hash: None,
                });
                if !call_id.is_empty() {
                    call_id_map.insert(call_id, idx);
                }
            }
            "tool.execution_complete" => {
                let call_id = data
                    .get("toolCallId")
                    .and_then(|id| id.as_str())
                    .unwrap_or("");
                if call_id.is_empty() {
                    continue;
                }
                if let Some(&idx) = call_id_map.get(call_id) {
                    if let Some(msg) = messages.get_mut(idx) {
                        let result_content = data
                            .get("result")
                            .and_then(|r| r.get("content"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        if !result_content.is_empty() {
                            msg.content = result_content.to_string();
                        }
                    }
                }
            }
            // Skip: session.info, assistant.turn_start, assistant.turn_end,
            // subagent.started, subagent.completed, system.notification,
            // session.mode_changed, session.task_complete, hook.*
            _ => continue,
        }
    }

    if messages.is_empty() {
        return None;
    }

    // Session ID from directory name: ~/.copilot/session-state/{sessionId}/events.jsonl
    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });

    let project_path = cwd.clone().unwrap_or_default();
    let project_name = project_name_from_path(&project_path);
    let title = session_title(first_user_message.as_deref());
    let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());
    let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());
    let content_text = truncate_to_bytes(&content_parts.join(" "), FTS_CONTENT_LIMIT);
    let message_count = messages.len() as u32;

    Some(ParsedSession {
        meta: crate::models::SessionMeta {
            id: session_id,
            provider: Provider::Copilot,
            title,
            project_path,
            project_name,
            created_at,
            updated_at,
            message_count,
            file_size_bytes: file_size,
            source_path: path.to_string_lossy().to_string(),
            is_sidechain: false,
            variant_name: None,
            model: None,
            cc_version,
            git_branch,
            parent_id: None,
        },
        messages,
        content_text,
    })
}
