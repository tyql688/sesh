use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::models::{Message, MessageRole, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT,
};

use super::images::{
    contains_image_placeholder_without_source, contains_image_source,
    merge_image_placeholders_with_sources,
};
use crate::models::Provider;

/// Shared mutable state threaded through the per-message-type handlers.
struct ParseState {
    messages: Vec<Message>,
    content_parts: Vec<String>,
    first_user_message: Option<String>,
    pending_user_message: Option<(String, Option<String>)>,
    tool_use_id_map: HashMap<String, usize>,
}

/// Extract parent session ID from subagent path.
/// Path pattern: .../{parent_session_id}/subagents/agent-{agentId}.jsonl
fn parent_id_from_path(path: &Path) -> Option<String> {
    let parent = path.parent()?; // subagents/
    if parent.file_name()?.to_str()? != "subagents" {
        return None;
    }
    let session_dir = parent.parent()?; // {parent_session_id}/
    Some(session_dir.file_name()?.to_str()?.to_string())
}

pub fn parse_session_file(path: &PathBuf) -> Option<ParsedSession> {
    let file = File::open(path).ok()?;
    let metadata = fs::metadata(path).ok()?;
    let file_size = metadata.len();

    let reader = BufReader::new(file);
    let mut state = ParseState {
        messages: Vec::new(),
        content_parts: Vec::new(),
        first_user_message: None,
        pending_user_message: None,
        tool_use_id_map: HashMap::new(),
    };
    let mut summary_text: Option<String> = None;
    let mut custom_title: Option<String> = None;
    let mut ai_title: Option<String> = None;
    let mut first_timestamp: Option<String> = None;
    let mut last_timestamp: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut is_sidechain = false;
    let parent_id = parent_id_from_path(path);
    let subagent_title = parent_id.as_ref().and_then(|_| {
        let meta_path = path.with_extension("meta.json");
        let meta_content = fs::read_to_string(&meta_path).ok()?;
        let meta_json: Value = serde_json::from_str(&meta_content).ok()?;
        meta_json
            .get("description")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string())
    });
    let mut model: Option<String> = None;
    let mut cc_version: Option<String> = None;
    let mut git_branch: Option<String> = None;

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

        let line_type = match entry.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        // Extract cwd from the first message that has it
        if cwd.is_none() {
            if let Some(c) = entry.get("cwd").and_then(|c| c.as_str()) {
                if !c.is_empty() {
                    cwd = Some(c.to_string());
                }
            }
        }

        // Detect sidechain sessions (subagent messages)
        if !is_sidechain
            && entry
                .get("isSidechain")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            is_sidechain = true;
        }

        // Extract cc_version from the first entry that has it
        if cc_version.is_none() {
            if let Some(v) = entry.get("version").and_then(|v| v.as_str()) {
                if !v.is_empty() {
                    cc_version = Some(v.to_string());
                }
            }
        }

        // Extract git_branch from the first entry that has it (filter out "HEAD")
        if git_branch.is_none() {
            if let Some(b) = entry.get("gitBranch").and_then(|b| b.as_str()) {
                if !b.is_empty() && b != "HEAD" {
                    git_branch = Some(b.to_string());
                }
            }
        }

        // Extract timestamp
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

        match line_type.as_str() {
            "user" => {
                handle_user_message(&entry, &mut state, timestamp);
            }
            "assistant" => {
                // Extract model from the first assistant message that has it
                if model.is_none() {
                    if let Some(m) = entry
                        .get("message")
                        .and_then(|msg| msg.get("model"))
                        .and_then(|m| m.as_str())
                    {
                        if !m.is_empty() {
                            model = Some(m.to_string());
                        }
                    }
                }
                handle_assistant_message(&entry, &mut state, timestamp);
            }
            "summary" => {
                handle_summary(&entry, &mut summary_text, &mut state);
                continue;
            }
            "system" => {
                handle_system_message(&entry, &mut state, timestamp);
            }
            "custom-title" => {
                flush_pending(&mut state);
                if let Some(t) = entry.get("title").and_then(|t| t.as_str()) {
                    if !t.trim().is_empty() {
                        custom_title = Some(t.to_string());
                    }
                }
                continue;
            }
            "ai-title" => {
                flush_pending(&mut state);
                if let Some(t) = entry.get("title").and_then(|t| t.as_str()) {
                    if !t.trim().is_empty() {
                        ai_title = Some(t.to_string());
                    }
                }
                continue;
            }
            // Skip all other types
            _ => {
                flush_pending(&mut state);
                continue;
            }
        }
    }

    flush_pending(&mut state);

    // Subagent files detected by path are always sidechains
    let is_sidechain = is_sidechain || parent_id.is_some();

    if state.messages.is_empty() {
        return None;
    }

    let session_id = path.file_stem()?.to_string_lossy().to_string();

    let project_path = cwd.unwrap_or_default();
    let project_name = project_name_from_path(&project_path);

    let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

    let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

    let full_content = state.content_parts.join("\n");
    let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);

    let title = custom_title
        .or(ai_title)
        .or(subagent_title)
        .unwrap_or_else(|| {
            session_title(
                state
                    .first_user_message
                    .as_deref()
                    .or(summary_text.as_deref()),
            )
        });

    let meta = crate::models::SessionMeta {
        id: session_id,
        provider: Provider::Claude,
        title,
        project_path,
        project_name,
        created_at,
        updated_at,
        message_count: state.messages.len() as u32,
        file_size_bytes: file_size,
        source_path: path.to_string_lossy().to_string(),
        is_sidechain,
        variant_name: None,
        model,
        cc_version,
        git_branch,
        parent_id,
    };

    Some(ParsedSession {
        meta,
        messages: state.messages,
        content_text,
    })
}

/// Handle a "user" line, which may be a real user message or a tool_result turn.
fn handle_user_message(entry: &Value, state: &mut ParseState, timestamp: Option<String>) {
    let msg = match entry.get("message") {
        Some(m) => m,
        None => return,
    };

    // Check if this "user" entry is actually a tool_result
    // (the Anthropic API sends tool results as user-role turns)
    if is_tool_result_message(msg) {
        handle_tool_result(msg, state, &timestamp);
        return;
    }

    let text = extract_message_content(msg);
    if text.trim().is_empty() {
        return;
    }
    let is_meta = entry
        .get("isMeta")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if let Some((pending_text, pending_timestamp)) = state.pending_user_message.take() {
        if is_meta
            && contains_image_placeholder_without_source(&pending_text)
            && contains_image_source(&text)
        {
            append_user_message(
                &mut state.messages,
                &mut state.content_parts,
                &mut state.first_user_message,
                merge_image_placeholders_with_sources(&pending_text, &text),
                pending_timestamp,
            );
            return;
        }

        append_user_message(
            &mut state.messages,
            &mut state.content_parts,
            &mut state.first_user_message,
            pending_text,
            pending_timestamp,
        );
    }

    if contains_image_placeholder_without_source(&text) {
        state.pending_user_message = Some((text, timestamp));
        return;
    }

    if !is_meta || contains_image_source(&text) {
        append_user_message(
            &mut state.messages,
            &mut state.content_parts,
            &mut state.first_user_message,
            text,
            timestamp,
        );
    }
}

/// Merge tool_result blocks from a user-role turn into their matching tool_use messages.
fn handle_tool_result(msg: &Value, state: &mut ParseState, timestamp: &Option<String>) {
    flush_pending(state);
    // Merge each tool_result into its matching tool_use message
    if let Some(Value::Array(arr)) = msg.get("content") {
        for result_item in arr {
            if result_item.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                continue;
            }
            let result_text = extract_tool_result_content(result_item);
            if result_text.trim().is_empty() {
                continue;
            }
            state.content_parts.push(result_text.clone());
            let use_id = result_item.get("tool_use_id").and_then(|i| i.as_str());
            if let Some(idx) = use_id.and_then(|id| state.tool_use_id_map.get(id)) {
                // Merge result into the existing tool_use message
                state.messages[*idx].content = result_text;
            } else {
                // No matching tool_use found -- emit as standalone
                state.messages.push(Message {
                    role: MessageRole::Tool,
                    content: result_text,
                    timestamp: timestamp.clone(),
                    tool_name: use_id.map(std::string::ToString::to_string),
                    tool_input: None,
                    token_usage: None,
                    model: None,
                });
            }
        }
    }
}

/// Handle an "assistant" line: split content into text, thinking, and tool_use messages.
fn handle_assistant_message(entry: &Value, state: &mut ParseState, timestamp: Option<String>) {
    flush_pending(state);
    let msg = match entry.get("message") {
        Some(m) => m,
        None => return,
    };

    // Extract per-message model
    let per_message_model = msg
        .get("model")
        .and_then(|m| m.as_str())
        .map(|s| s.to_string());

    // Extract token usage for this assistant turn
    let turn_usage = extract_token_usage(msg);
    let turn_start = state.messages.len();

    // Split assistant messages: text parts as assistant, tool_use as tool
    if let Some(Value::Array(arr)) = msg.get("content") {
        let mut text_parts = Vec::new();
        for item in arr {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match item_type {
                "thinking" => {
                    if let Some(t) = item.get("thinking").and_then(|t| t.as_str()) {
                        if !t.trim().is_empty() {
                            // Emit thinking as a separate assistant message with marker
                            state.messages.push(Message {
                                role: MessageRole::System,
                                content: format!("[thinking]\n{t}"),
                                timestamp: timestamp.clone(),
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                                model: None,
                            });
                        }
                    }
                }
                "text" => {
                    if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                        if !t.trim().is_empty() {
                            text_parts.push(t.to_string());
                        }
                    }
                }
                "tool_use" => {
                    // Flush accumulated text as assistant message
                    if !text_parts.is_empty() {
                        let text = text_parts.join("\n");
                        state.content_parts.push(text.clone());
                        state.messages.push(Message {
                            role: MessageRole::Assistant,
                            content: text,
                            timestamp: timestamp.clone(),
                            tool_name: None,
                            tool_input: None,
                            token_usage: None,
                            model: per_message_model.clone(),
                        });
                        text_parts.clear();
                    }
                    // Emit tool_use as a Tool message
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                    let input = item.get("input").map(std::string::ToString::to_string);
                    let msg_idx = state.messages.len();
                    state.messages.push(Message {
                        role: MessageRole::Tool,
                        content: String::new(),
                        timestamp: timestamp.clone(),
                        tool_name: Some(name.to_string()),
                        tool_input: input,
                        token_usage: None,
                        model: None,
                    });
                    // Record tool_use_id for merging results later
                    if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                        state.tool_use_id_map.insert(id.to_string(), msg_idx);
                    }
                }
                _ => {}
            }
        }
        // Flush remaining text
        if !text_parts.is_empty() {
            let text = text_parts.join("\n");
            state.content_parts.push(text.clone());
            state.messages.push(Message {
                role: MessageRole::Assistant,
                content: text,
                timestamp,
                tool_name: None,
                tool_input: None,
                token_usage: None,
                model: per_message_model,
            });
        }
    } else {
        // content is a plain string
        let text = extract_message_content(msg);
        if !text.trim().is_empty() {
            state.content_parts.push(text.clone());
            state.messages.push(Message {
                role: MessageRole::Assistant,
                content: text,
                timestamp,
                tool_name: None,
                tool_input: None,
                token_usage: None,
                model: per_message_model,
            });
        }
    }

    // Attach token usage to the last assistant/tool message of this turn
    if let Some(usage) = turn_usage {
        // Find the last non-thinking message in this turn
        if let Some(last_msg) = state.messages[turn_start..]
            .iter_mut()
            .filter(|m| m.role != MessageRole::System)
            .last()
        {
            last_msg.token_usage = Some(usage);
        }
    }
}

/// Handle a "summary" line: capture the first non-empty summary text.
fn handle_summary(entry: &Value, summary_text: &mut Option<String>, state: &mut ParseState) {
    if summary_text.is_none() {
        if let Some(s) = entry.get("summary").and_then(|s| s.as_str()) {
            if !s.trim().is_empty() {
                *summary_text = Some(s.to_string());
            }
        }
    }
    flush_pending(state);
}

/// Handle a "system" line: emit human-readable summaries of system subtypes.
fn handle_system_message(entry: &Value, state: &mut ParseState, timestamp: Option<String>) {
    flush_pending(state);

    let subtype = match entry.get("subtype").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return,
    };

    let content = match subtype {
        "turn_duration" => {
            let duration_ms = entry
                .get("durationMs")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let message_count = entry
                .get("messageCount")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!(
                "[turn_duration] {:.1}s, {} messages",
                duration_ms / 1000.0,
                message_count
            )
        }
        "compact_boundary" => {
            let pre_tokens = entry
                .get("compactMetadata")
                .and_then(|m| m.get("preTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if pre_tokens < 1000 {
                format!("[compact_boundary] {} tokens", pre_tokens)
            } else {
                format!(
                    "[compact_boundary] {:.1}k tokens",
                    pre_tokens as f64 / 1000.0
                )
            }
        }
        "microcompact_boundary" => {
            let metadata = entry.get("microcompactMetadata");
            let pre_tokens = metadata
                .and_then(|m| m.get("preTokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tokens_saved = metadata
                .and_then(|m| m.get("tokensSaved"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!(
                "[microcompact_boundary] {:.1}k tokens saved {:.1}k",
                pre_tokens as f64 / 1000.0,
                tokens_saved as f64 / 1000.0
            )
        }
        "stop_hook_summary" => {
            let hook_count = entry.get("hookCount").and_then(|v| v.as_u64()).unwrap_or(0);
            let hook_details: Vec<String> = entry
                .get("hookInfos")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|h| {
                            let cmd = h
                                .get("command")
                                .and_then(|c| c.as_str())
                                .unwrap_or("unknown");
                            let ms = h.get("durationMs").and_then(|d| d.as_u64()).unwrap_or(0);
                            format!("{cmd} ({ms}ms)")
                        })
                        .collect()
                })
                .unwrap_or_default();
            format!(
                "[stop_hook_summary] {} hooks: {}",
                hook_count,
                hook_details.join(", ")
            )
        }
        "api_error" => {
            let code = entry
                .get("cause")
                .and_then(|c| c.get("code"))
                .and_then(|c| c.as_str())
                .unwrap_or("Unknown");
            let retry = entry
                .get("retryAttempt")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let max_retries = entry
                .get("maxRetries")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("[api_error] {code} (retry {retry}/{max_retries})")
        }
        _ => return,
    };

    state.messages.push(Message {
        role: MessageRole::System,
        content,
        timestamp,
        tool_name: None,
        tool_input: None,
        token_usage: None,
        model: None,
    });
}

/// Flush any pending user message that was waiting for an image-source merge.
fn flush_pending(state: &mut ParseState) {
    if let Some((text, timestamp)) = state.pending_user_message.take() {
        append_user_message(
            &mut state.messages,
            &mut state.content_parts,
            &mut state.first_user_message,
            text,
            timestamp,
        );
    }
}

fn append_user_message(
    messages: &mut Vec<Message>,
    content_parts: &mut Vec<String>,
    first_user_message: &mut Option<String>,
    text: String,
    timestamp: Option<String>,
) {
    if text.trim().is_empty() {
        return;
    }

    let trimmed = text.trim_start();
    if is_system_content(trimmed) {
        return;
    }

    if first_user_message.is_none() {
        *first_user_message = Some(text.clone());
    }

    content_parts.push(text.clone());
    messages.push(Message {
        role: MessageRole::User,
        content: text,
        timestamp,
        tool_name: None,
        tool_input: None,
        token_usage: None,
        model: None,
    });
}

/// Extract token usage from a message's `usage` field.
fn extract_token_usage(message: &Value) -> Option<TokenUsage> {
    let usage = message.get("usage")?;
    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_creation_input_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_read_input_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }
    Some(TokenUsage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens,
        cache_read_input_tokens,
    })
}

/// Extract text content from a message object.
/// The `content` field can be a string or an array of typed blocks.
/// Handles both "text" and "tool_use" content blocks.
fn extract_message_content(message: &Value) -> String {
    let content = message.get("content");
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            let mut parts = Vec::new();
            for item in arr {
                let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match item_type {
                    "text" => {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            parts.push(text.to_string());
                        }
                    }
                    "tool_use" => {
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                        let input = item
                            .get("input")
                            .map(std::string::ToString::to_string)
                            .unwrap_or_default();
                        let end = if input.len() > 200 {
                            input.floor_char_boundary(200)
                        } else {
                            input.len()
                        };
                        parts.push(format!("[Tool: {}] {}", name, &input[..end]));
                    }
                    "tool_result" => {
                        if let Some(text) = item.get("content").and_then(|c| c.as_str()) {
                            let end = if text.len() > 200 {
                                text.floor_char_boundary(200)
                            } else {
                                text.len()
                            };
                            parts.push(format!("[Result] {}", &text[..end]));
                        }
                    }
                    // "image" blocks are handled by the isMeta merge logic:
                    // text has [Image #N] placeholders, next isMeta entry provides
                    // file paths via [Image: source: /path], and
                    // merge_image_placeholders_with_sources combines them.
                    _ => {}
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Check if a "user" message is actually a tool_result turn.
/// In the Anthropic API, tool results are sent as user-role messages
/// with content blocks of type "tool_result".
fn is_tool_result_message(message: &Value) -> bool {
    match message.get("content") {
        Some(Value::Array(arr)) if !arr.is_empty() => arr
            .iter()
            .all(|item| item.get("type").and_then(|t| t.as_str()) == Some("tool_result")),
        _ => false,
    }
}

/// Resolve `<persisted-output>` tags by reading the referenced external file.
/// Falls back to keeping the original content (with preview) if the file can't be read.
/// Only paths under `~/.claude/` are allowed to prevent arbitrary file reads.
pub fn resolve_persisted_outputs(content: &str) -> String {
    const TAG_START: &str = "<persisted-output>";
    const TAG_END: &str = "</persisted-output>";

    if !content.contains(TAG_START) {
        return content.to_string();
    }

    let mut result = String::new();
    let mut remaining = content;

    while let Some(start_pos) = remaining.find(TAG_START) {
        // Add everything before the tag
        result.push_str(&remaining[..start_pos]);

        let after_tag_start = &remaining[start_pos + TAG_START.len()..];
        if let Some(end_pos) = after_tag_start.find(TAG_END) {
            let inner = &after_tag_start[..end_pos];

            // Extract file path from "Full output saved to: /path"
            let file_content = inner
                .lines()
                .find_map(|line| {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("Full output saved to: ") {
                        Some(rest.trim().to_string())
                    } else if trimmed.contains("saved to: ") {
                        trimmed
                            .split("saved to: ")
                            .nth(1)
                            .map(|p| p.trim().to_string())
                    } else {
                        None
                    }
                })
                .and_then(|path| {
                    // Only allow reading files under ~/.claude/ to prevent arbitrary file access
                    let canonical = std::fs::canonicalize(&path).ok()?;
                    let claude_dir = dirs::home_dir()?.join(".claude");
                    let claude_canonical = std::fs::canonicalize(&claude_dir).ok()?;
                    if !canonical.starts_with(&claude_canonical) {
                        return None;
                    }
                    std::fs::read_to_string(&canonical).ok()
                });

            match file_content {
                Some(full) => result.push_str(&full),
                None => {
                    // Keep the original tag content as fallback
                    result.push_str(TAG_START);
                    result.push_str(inner);
                    result.push_str(TAG_END);
                }
            }

            remaining = &after_tag_start[end_pos + TAG_END.len()..];
        } else {
            // No closing tag found, keep everything as-is
            result.push_str(&remaining[start_pos..]);
            remaining = "";
        }
    }

    result.push_str(remaining);
    result
}

/// Extract text content from a single tool_result block.
/// The `content` field can be a string, an array of text blocks, or absent.
fn extract_tool_result_content(result: &Value) -> String {
    match result.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            let mut parts = Vec::new();
            for item in arr {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                            parts.push(t.to_string());
                        }
                    }
                    Some("image") => {
                        // Inline base64 image as data URI for frontend rendering
                        let source = item.get("source");
                        let data = source.and_then(|s| s.get("data")).and_then(|d| d.as_str());
                        let media = source
                            .and_then(|s| s.get("media_type"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("image/png");
                        if let Some(b64) = data {
                            parts.push(format!("[Image: source: data:{};base64,{}]", media, b64));
                        } else {
                            parts.push("[Image]".to_string());
                        }
                    }
                    _ => {}
                }
            }
            parts.join("\n")
        }
        _ => String::new(),
    }
}
