use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT, NO_PROJECT,
};

use super::tools::*;
use super::CodexProvider;

#[derive(Clone, Debug)]
pub struct CodexUsageEvent {
    pub timestamp: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Deserialize)]
struct CodexLine {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    line_type: String,
    payload: Option<Value>,
}

struct PendingCodexUserMessage {
    content: String,
    timestamp: Option<String>,
    image_segments: Vec<String>,
}

impl CodexProvider {
    pub fn parse_session_file(&self, path: &PathBuf) -> Option<ParsedSession> {
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
        let mut call_id_map: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut model: Option<String> = None;
        let mut model_provider: Option<String> = None;
        let mut current_model: Option<String> = None;
        let mut cc_version: Option<String> = None;
        let mut git_branch: Option<String> = None;
        let mut is_sidechain = false;
        let mut parent_id: Option<String> = None;
        let mut agent_nickname: Option<String> = None;
        let mut pending_user_message: Option<PendingCodexUserMessage> = None;
        // When parsing subagent files, skip the forked parent context.
        // Subagent JSONL: [sub_meta, parent_meta, ...parent_context..., spawn_marker, sub_turn]
        // The marker "You are the newly spawned agent" signals end of parent context.
        let mut skipping_fork_context = false;

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

            // Skip forked parent context in subagent files.
            // End marker: function_call_output containing "newly spawned agent".
            if skipping_fork_context {
                if entry.line_type == "response_item" {
                    let item_type = payload.get("type").and_then(|v| v.as_str());
                    if item_type == Some("function_call_output") {
                        let output = payload.get("output").and_then(|v| v.as_str()).unwrap_or("");
                        if output.contains("newly spawned agent") {
                            skipping_fork_context = false;
                        }
                    }
                }
                continue;
            }

            match entry.line_type.as_str() {
                "session_meta" => {
                    // Only process the first session_meta; subagent JSONL files
                    // contain a second session_meta for the parent context which
                    // would overwrite the subagent's own id/cwd/source fields.
                    if session_id.is_some() {
                        // 2nd session_meta = start of forked parent context
                        if is_sidechain {
                            skipping_fork_context = true;
                        }
                        continue;
                    }
                    if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                        session_id = Some(id.to_string());
                    }
                    if let Some(c) = payload.get("cwd").and_then(|v| v.as_str()) {
                        cwd = Some(c.to_string());
                    }
                    if let Some(v) = payload.get("cli_version").and_then(|v| v.as_str()) {
                        if !v.is_empty() {
                            cc_version = Some(v.to_string());
                        }
                    }
                    if let Some(m) = payload.get("model_provider").and_then(|v| v.as_str()) {
                        if !m.is_empty() {
                            model_provider = Some(m.to_string());
                        }
                    }
                    if let Some(b) = payload
                        .get("git")
                        .and_then(|g| g.get("branch"))
                        .and_then(|v| v.as_str())
                    {
                        if !b.is_empty() && b != "HEAD" {
                            git_branch = Some(b.to_string());
                        }
                    }
                    // Detect subagent sessions: source.subagent.thread_spawn
                    if let Some(spawn) = payload
                        .get("source")
                        .and_then(|s| s.get("subagent"))
                        .and_then(|a| a.get("thread_spawn"))
                    {
                        is_sidechain = true;
                        parent_id = spawn
                            .get("parent_thread_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        agent_nickname = payload
                            .get("agent_nickname")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "response_item" => {
                    // Skip developer role and reasoning type
                    let role_str = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let item_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

                    if role_str == "developer" || item_type == "reasoning" {
                        continue;
                    }

                    if !(item_type == "message" && role_str == "user") {
                        flush_pending_user_message(
                            &mut pending_user_message,
                            &mut messages,
                            &mut content_parts,
                            &mut first_user_message,
                        );
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

                            if role == MessageRole::User {
                                let image_segments = extract_image_source_segments(&text);
                                flush_pending_user_message(
                                    &mut pending_user_message,
                                    &mut messages,
                                    &mut content_parts,
                                    &mut first_user_message,
                                );
                                pending_user_message = Some(PendingCodexUserMessage {
                                    content: text,
                                    timestamp: entry.timestamp.clone(),
                                    image_segments,
                                });
                                continue;
                            }

                            let msg_model = if role == MessageRole::Assistant {
                                current_model.clone()
                            } else {
                                None
                            };
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
                                model: msg_model,
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
                                        .and_then(|v| {
                                            v.get("path")
                                                .and_then(|p| p.as_str())
                                                .map(|s| s.to_string())
                                        })
                                    {
                                        messages.push(Message {
                                            role: MessageRole::Assistant,
                                            content: format!("[Image: source: {path}]"),
                                            timestamp: entry.timestamp.clone(),
                                            tool_name: None,
                                            tool_input: None,
                                            token_usage: None,
                                            model: None,
                                        });
                                        continue;
                                    }
                                    None
                                }
                                "write_stdin" => {
                                    // Skip empty stdin writes (just polling)
                                    let is_empty = arguments_str
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                        .and_then(|v| {
                                            v.get("chars")
                                                .and_then(|c| c.as_str())
                                                .map(|s| s.is_empty())
                                        })
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
                                model: None,
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
                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied()
                            {
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
                                model: None,
                            });
                        }
                        "custom_tool_call" => {
                            let raw_name = payload
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("tool");
                            let display_name = map_codex_tool_name(raw_name);
                            let input = payload.get("input").map(|v| {
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
                                model: None,
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
                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied()
                            {
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
                                    model: None,
                                });
                            }
                        }
                        _ => continue,
                    }
                }
                "turn_context" => {
                    flush_pending_user_message(
                        &mut pending_user_message,
                        &mut messages,
                        &mut content_parts,
                        &mut first_user_message,
                    );
                    // Extract actual model name (e.g. "gpt-5.4") from turn_context
                    if let Some(m) = payload.get("model").and_then(|v| v.as_str()) {
                        if !m.is_empty() {
                            current_model = Some(m.to_string());
                            if model.is_none() {
                                model = Some(m.to_string());
                            }
                        }
                    }
                }
                "event_msg" => {
                    let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    // agent_message is a duplicate of response_item/message/assistant — skip
                    match event_type {
                        "user_message" => {
                            let pending = pending_user_message.take();
                            let fallback_content =
                                pending.as_ref().map(|message| message.content.clone());
                            let response_image_segments = pending
                                .as_ref()
                                .map(|message| message.image_segments.clone())
                                .unwrap_or_default();
                            let timestamp = entry
                                .timestamp
                                .clone()
                                .or_else(|| pending.and_then(|message| message.timestamp));
                            let built_content =
                                build_codex_user_message(payload, &response_image_segments);
                            let content = if built_content.is_empty() {
                                fallback_content.unwrap_or_default()
                            } else {
                                built_content
                            };
                            append_user_message(
                                &mut messages,
                                &mut content_parts,
                                &mut first_user_message,
                                content,
                                timestamp,
                            );
                        }
                        "token_count" => {
                            if let Some(info) = payload.get("info") {
                                if let Some(last) = info.get("last_token_usage") {
                                    let input = last
                                        .get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                    let output = last
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                    let cached = last
                                        .get("cached_input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                    let usage = TokenUsage {
                                        input_tokens: input,
                                        output_tokens: output,
                                        cache_read_input_tokens: cached,
                                        cache_creation_input_tokens: 0,
                                    };
                                    if let Some(last_msg) = messages
                                        .iter_mut()
                                        .rev()
                                        .find(|m| m.role == MessageRole::Assistant)
                                    {
                                        last_msg.token_usage = Some(usage);
                                    }
                                }
                            }
                        }
                        _ => {
                            flush_pending_user_message(
                                &mut pending_user_message,
                                &mut messages,
                                &mut content_parts,
                                &mut first_user_message,
                            );
                        }
                    }
                }
                _ => continue,
            }
        }

        flush_pending_user_message(
            &mut pending_user_message,
            &mut messages,
            &mut content_parts,
            &mut first_user_message,
        );

        if messages.is_empty() {
            return None;
        }

        // Session ID: from session_meta payload.id, fallback to filename parsing
        let session_id = session_id.unwrap_or_else(|| {
            path.file_stem().map_or_else(
                || "unknown".to_string(),
                |s| s.to_string_lossy().to_string(),
            )
        });

        let title = agent_nickname
            .as_deref()
            .map(|n| n.to_string())
            .unwrap_or_else(|| session_title(first_user_message.as_deref()));

        let project_path = cwd.unwrap_or_else(|| NO_PROJECT.to_string());

        let project_name = project_name_from_path(&project_path);

        let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

        let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

        let full_content = content_parts.join("\n");
        let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);

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
            is_sidechain,
            variant_name: None,
            model: model.or(model_provider),
            cc_version,
            git_branch,
            parent_id,
        };

        Some(ParsedSession {
            meta,
            messages,
            content_text,
        })
    }
}

pub fn extract_usage_events_from_file(path: &PathBuf) -> Vec<CodexUsageEvent> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let reader = BufReader::new(file);

    let mut current_model: Option<String> = None;
    let mut previous_totals: Option<(u64, u64, u64, u64, u64)> = None;
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) if !line.trim().is_empty() => line,
            _ => continue,
        };

        let entry: CodexLine = match serde_json::from_str(&line) {
            Ok(entry) => entry,
            Err(_) => continue,
        };

        let Some(payload) = entry.payload.as_ref() else {
            continue;
        };

        match entry.line_type.as_str() {
            "turn_context" => {
                current_model = extract_codex_model(payload).or(current_model);
            }
            "event_msg" => {
                if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
                    continue;
                }
                let Some(timestamp) = entry.timestamp.clone() else {
                    continue;
                };
                let Some(info) = payload.get("info") else {
                    continue;
                };

                let last_usage = info
                    .get("last_token_usage")
                    .and_then(normalize_codex_raw_usage);
                let total_usage = info
                    .get("total_token_usage")
                    .and_then(normalize_codex_raw_usage);

                let raw_usage = match (last_usage, total_usage) {
                    (Some(last), _) => {
                        previous_totals = Some(last.1);
                        Some(last)
                    }
                    (None, Some(total)) => {
                        let delta = subtract_codex_usage(total.1, previous_totals);
                        previous_totals = Some(total.1);
                        Some((total.0, delta))
                    }
                    (None, None) => None,
                };

                let Some((model, (input, cached, output, reasoning, total))) = raw_usage else {
                    continue;
                };
                if input == 0 && cached == 0 && output == 0 && reasoning == 0 && total == 0 {
                    continue;
                }

                let model = extract_codex_model(info)
                    .or_else(|| extract_codex_model(payload))
                    .or_else(|| current_model.clone())
                    .unwrap_or_else(|| model.unwrap_or_else(|| "gpt-5".to_string()));
                current_model = Some(model.clone());

                let cache_read = cached.min(input);
                let non_cached_input = input.saturating_sub(cache_read);

                events.push(CodexUsageEvent {
                    timestamp,
                    model,
                    input_tokens: non_cached_input,
                    output_tokens: output,
                    cache_read_input_tokens: cache_read,
                });
            }
            _ => {}
        }
    }

    events
}

fn extract_codex_model(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("info")
                .and_then(|info| info.get("model"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("info")
                .and_then(|info| info.get("model_name"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("metadata")
                .and_then(|meta| meta.get("model"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

type RawCodexUsage = (Option<String>, (u64, u64, u64, u64, u64));

fn normalize_codex_raw_usage(value: &Value) -> Option<RawCodexUsage> {
    let input = value
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cached = value
        .get("cached_input_tokens")
        .or_else(|| value.get("cache_read_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = value
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning = value
        .get("reasoning_output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = value
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| input + output);
    let model = value
        .get("model")
        .or_else(|| value.get("model_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((model, (input, cached, output, reasoning, total)))
}

fn subtract_codex_usage(
    current: (u64, u64, u64, u64, u64),
    previous: Option<(u64, u64, u64, u64, u64)>,
) -> (u64, u64, u64, u64, u64) {
    let prev = previous.unwrap_or((0, 0, 0, 0, 0));
    (
        current.0.saturating_sub(prev.0),
        current.1.saturating_sub(prev.1),
        current.2.saturating_sub(prev.2),
        current.3.saturating_sub(prev.3),
        current.4.saturating_sub(prev.4),
    )
}

fn append_user_message(
    messages: &mut Vec<Message>,
    content_parts: &mut Vec<String>,
    first_user_message: &mut Option<String>,
    content: String,
    timestamp: Option<String>,
) {
    if content.is_empty() {
        return;
    }

    let normalized_text = strip_inline_image_sources(&content);
    let trimmed = normalized_text.trim_start();
    if is_system_content(trimmed) {
        return;
    }

    if first_user_message.is_none() {
        *first_user_message = Some(normalized_text.clone());
    }

    if !normalized_text.is_empty() {
        content_parts.push(normalized_text);
    }

    messages.push(Message {
        role: MessageRole::User,
        content,
        timestamp,
        tool_name: None,
        tool_input: None,
        token_usage: None,
        model: None,
    });
}

fn flush_pending_user_message(
    pending_user_message: &mut Option<PendingCodexUserMessage>,
    messages: &mut Vec<Message>,
    content_parts: &mut Vec<String>,
    first_user_message: &mut Option<String>,
) {
    let Some(pending) = pending_user_message.take() else {
        return;
    };

    append_user_message(
        messages,
        content_parts,
        first_user_message,
        pending.content,
        pending.timestamp,
    );
}

#[cfg(test)]
mod tests {
    use super::extract_usage_events_from_file;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn extract_usage_events_splits_cached_input_from_input() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("codex.jsonl");
        fs::write(
            &file,
            concat!(
                "{\"timestamp\":\"2026-04-10T10:00:00Z\",\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":1000,\"cached_input_tokens\":600,\"output_tokens\":50,\"reasoning_output_tokens\":25,\"total_tokens\":1050}}}}\n"
            ),
        )
        .unwrap();

        let events = extract_usage_events_from_file(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, "gpt-5.4");
        assert_eq!(events[0].input_tokens, 400);
        assert_eq!(events[0].cache_read_input_tokens, 600);
        assert_eq!(events[0].output_tokens, 50);
    }
}
