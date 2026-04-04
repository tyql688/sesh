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

/// Map Qwen tool names to canonical CC Session names.
fn canonical_tool_name(name: &str) -> &str {
    match name {
        "run_shell_command" => "Bash",
        "edit" | "replace" => "Edit",
        "read_file" => "Read",
        "write_file" => "Write",
        "glob" => "Glob",
        "grep_search" | "search_file_content" => "Grep",
        "agent" | "task" => "Agent",
        "exit_plan_mode" => "Plan",
        "list_directory" => "Bash",
        "web_fetch" => "WebFetch",
        "web_search" => "WebSearch",
        other => other,
    }
}

/// Strip Qwen `@`-file references and "Content from referenced files" boilerplate
/// from user text parts, keeping only the actual user input.
fn clean_reference_boilerplate(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut in_ref_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "--- Content from referenced files ---" {
            in_ref_block = true;
            continue;
        }
        if trimmed == "--- End of content ---" {
            in_ref_block = false;
            continue;
        }
        if in_ref_block || trimmed.starts_with("Content from ") {
            continue;
        }
        // Strip @-file references (e.g. @../../../.qwen/tmp/clipboard/xxx.png)
        if trimmed.starts_with('@')
            && (trimmed.ends_with(".png")
                || trimmed.ends_with(".jpg")
                || trimmed.ends_with(".jpeg")
                || trimmed.ends_with(".gif")
                || trimmed.ends_with(".webp")
                || trimmed.ends_with(".svg"))
        {
            continue;
        }
        lines.push(line);
    }
    let result = lines.join("\n");
    result.trim().to_string()
}

/// Extract text content from a message's parts array.
fn extract_text_parts(parts: &[Value]) -> String {
    let has_inline_data = parts.iter().any(|p| p.get("inlineData").is_some());
    let mut texts = Vec::new();
    for part in parts {
        if part
            .get("thought")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue; // skip thinking parts in text extraction
        }
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            if !text.is_empty() {
                let cleaned = if has_inline_data {
                    clean_reference_boilerplate(text)
                } else {
                    text.to_string()
                };
                if !cleaned.is_empty() {
                    texts.push(cleaned);
                }
            }
        }
    }
    texts.join("\n")
}

/// Extract thinking content from parts with `thought: true`.
fn extract_thinking(parts: &[Value]) -> Option<String> {
    let mut thoughts = Vec::new();
    for part in parts {
        if part
            .get("thought")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                if !text.is_empty() {
                    thoughts.push(text);
                }
            }
        }
    }
    if thoughts.is_empty() {
        None
    } else {
        Some(thoughts.join("\n"))
    }
}

/// Extract image markers from inlineData parts.
fn extract_image_markers(parts: &[Value]) -> Vec<String> {
    let mut markers = Vec::new();
    for part in parts {
        if let Some(inline) = part.get("inlineData") {
            let mime = inline
                .get("mimeType")
                .and_then(|m| m.as_str())
                .unwrap_or("image/png");
            let data = inline.get("data").and_then(|d| d.as_str()).unwrap_or("");
            if !data.is_empty() {
                markers.push(format!("[Image: source: data:{mime};base64,{data}]"));
            }
        }
    }
    markers
}

/// Extract tool call info from functionCall parts.
fn extract_function_calls(parts: &[Value]) -> Vec<(String, String, String)> {
    // Returns: Vec<(call_id, tool_name, args_json)>
    let mut calls = Vec::new();
    for part in parts {
        if let Some(fc) = part.get("functionCall") {
            let id = fc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = fc
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = fc
                .get("args")
                .map(|a| serde_json::to_string(a).unwrap_or_default())
                .unwrap_or_default();
            calls.push((id, name, args));
        }
    }
    calls
}

/// Parse token usage from usageMetadata.
fn parse_usage(entry: &Value) -> Option<TokenUsage> {
    let usage = entry.get("usageMetadata")?;
    let input = usage
        .get("promptTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let output = usage
        .get("candidatesTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_read = usage
        .get("cachedContentTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    if input == 0 && output == 0 {
        return None;
    }
    Some(TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: cache_read,
    })
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
    let mut model: Option<String> = None;
    let mut cc_version: Option<String> = None;
    let mut git_branch: Option<String> = None;
    // Map call_id → index in messages vec for merging tool results
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

        let record_type = match entry.get("type").and_then(|t| t.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        // Extract cwd from first record that has it
        if cwd.is_none() {
            if let Some(c) = entry.get("cwd").and_then(|c| c.as_str()) {
                if !c.is_empty() {
                    cwd = Some(c.to_string());
                }
            }
        }

        // Extract version from first record
        if cc_version.is_none() {
            if let Some(v) = entry.get("version").and_then(|v| v.as_str()) {
                if !v.is_empty() {
                    cc_version = Some(v.to_string());
                }
            }
        }

        // Extract git branch
        if git_branch.is_none() {
            if let Some(b) = entry.get("gitBranch").and_then(|b| b.as_str()) {
                if !b.is_empty() && b != "HEAD" {
                    git_branch = Some(b.to_string());
                }
            }
        }

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

        match record_type.as_str() {
            "user" => {
                let parts = entry
                    .get("message")
                    .and_then(|m| m.get("parts"))
                    .and_then(|p| p.as_array());
                let parts = match parts {
                    Some(p) => p,
                    None => continue,
                };

                let text = extract_text_parts(parts);
                let image_markers = extract_image_markers(parts);

                let mut content = text.clone();
                for marker in &image_markers {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(marker);
                }

                if first_user_message.is_none() && !text.is_empty() {
                    first_user_message = Some(text.clone());
                }

                if !content.is_empty() {
                    content_parts.push(text);
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
            }
            "assistant" => {
                let parts = entry
                    .get("message")
                    .and_then(|m| m.get("parts"))
                    .and_then(|p| p.as_array());
                let parts = match parts {
                    Some(p) => p,
                    None => continue,
                };

                // Extract model from assistant record
                if model.is_none() {
                    if let Some(m) = entry.get("model").and_then(|m| m.as_str()) {
                        if !m.is_empty() {
                            model = Some(m.to_string());
                        }
                    }
                }
                let msg_model = entry
                    .get("model")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string());

                let token_usage = parse_usage(&entry);

                // Handle thinking
                if let Some(thinking) = extract_thinking(parts) {
                    messages.push(Message {
                        role: MessageRole::System,
                        content: format!("[thinking]\n{thinking}"),
                        timestamp: timestamp.clone(),
                        tool_name: None,
                        tool_input: None,
                        token_usage: None,
                        model: None,
                    });
                }

                // Handle text content
                let text = extract_text_parts(parts);
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
                        model: msg_model.clone(),
                    });
                }

                // Handle function calls
                let function_calls = extract_function_calls(parts);
                for (call_id, name, args) in &function_calls {
                    let canonical = canonical_tool_name(name);
                    let idx = messages.len();
                    messages.push(Message {
                        role: MessageRole::Tool,
                        content: String::new(), // filled by tool_result
                        timestamp: timestamp.clone(),
                        tool_name: Some(canonical.to_string()),
                        tool_input: Some(args.to_string()),
                        token_usage: None,
                        model: None,
                    });
                    if !call_id.is_empty() {
                        call_id_map.insert(call_id.to_string(), idx);
                    }
                }

                // If no text and no tool calls, but has token usage, attach to last assistant msg
                if !has_text && function_calls.is_empty() {
                    if let Some(usage) = token_usage {
                        // Find last assistant message and attach usage
                        for msg in messages.iter_mut().rev() {
                            if msg.role == MessageRole::Assistant {
                                if msg.token_usage.is_none() {
                                    msg.token_usage = Some(usage);
                                }
                                break;
                            }
                        }
                    }
                }
            }
            "tool_result" => {
                let parts = entry
                    .get("message")
                    .and_then(|m| m.get("parts"))
                    .and_then(|p| p.as_array());

                // Extract callId from toolCallResult
                let call_id = entry
                    .get("toolCallResult")
                    .and_then(|t| t.get("callId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Extract tool output from functionResponse
                let mut tool_output = String::new();
                let mut tool_name_from_result = String::new();
                if let Some(parts) = parts {
                    for part in parts {
                        if let Some(fr) = part.get("functionResponse") {
                            if let Some(name) = fr.get("name").and_then(|n| n.as_str()) {
                                tool_name_from_result = name.to_string();
                            }
                            if let Some(resp) = fr.get("response") {
                                if let Some(output) = resp.get("output").and_then(|o| o.as_str()) {
                                    tool_output = output.to_string();
                                } else {
                                    tool_output = serde_json::to_string(resp).unwrap_or_default();
                                }
                            }
                        }
                    }
                }

                // Merge into the matching tool call message
                if !call_id.is_empty() {
                    if let Some(&idx) = call_id_map.get(call_id) {
                        if let Some(msg) = messages.get_mut(idx) {
                            msg.content = tool_output;
                        }
                        continue;
                    }
                }

                // Fallback: no matching call_id, create standalone tool message
                if !tool_output.is_empty() || !tool_name_from_result.is_empty() {
                    let canonical = canonical_tool_name(&tool_name_from_result);
                    messages.push(Message {
                        role: MessageRole::Tool,
                        content: tool_output,
                        timestamp,
                        tool_name: Some(canonical.to_string()),
                        tool_input: None,
                        token_usage: None,
                        model: None,
                    });
                }
            }
            // Skip system records (ui_telemetry, slash_command, at_command, chat_compression)
            _ => continue,
        }
    }

    if messages.is_empty() {
        return None;
    }

    let session_id = path.file_stem()?.to_string_lossy().to_string();
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
            provider: Provider::Qwen,
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
            model,
            cc_version,
            git_branch,
            parent_id: None,
        },
        messages,
        content_text,
    })
}
