use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT, NO_PROJECT,
};
use crate::tool_metadata::{
    build_tool_metadata, enrich_tool_metadata, ToolCallFacts, ToolResultFacts,
};

use super::images::strip_at_image_refs;
use super::images::{looks_like_image_path, resolve_gemini_image_path};
use super::tools::normalize_gemini_message;
use super::{ChatMessage, ChatSession, GeminiProvider};

#[derive(Default)]
struct JsonlMetadata {
    session_id: Option<String>,
    start_time: Option<String>,
    last_updated: Option<String>,
    kind: Option<String>,
    summary: Option<String>,
}

impl JsonlMetadata {
    fn apply(&mut self, record: &Value) {
        let source = record.get("$set").unwrap_or(record);

        if let Some(session_id) = source.get("sessionId").and_then(|v| v.as_str()) {
            self.session_id = Some(session_id.to_string());
        }
        if let Some(start_time) = source.get("startTime").and_then(|v| v.as_str()) {
            self.start_time = Some(start_time.to_string());
        }
        if let Some(last_updated) = source.get("lastUpdated").and_then(|v| v.as_str()) {
            self.last_updated = Some(last_updated.to_string());
        }
        if let Some(kind) = source.get("kind").and_then(|v| v.as_str()) {
            self.kind = Some(kind.to_string());
        }
        if let Some(summary) = source.get("summary").and_then(|v| v.as_str()) {
            self.summary = Some(summary.to_string());
        }
    }
}

fn gemini_parent_id_from_subagent_path(path: &Path) -> Option<String> {
    let parent_dir = path.parent()?;
    if parent_dir.parent()?.file_name()?.to_str()? != "chats" {
        return None;
    }
    parent_dir.file_name()?.to_str().map(str::to_string)
}

fn gemini_tool_result_value(tc: &Value, result_text: &str) -> Option<Value> {
    let response = tc
        .get("result")
        .and_then(|r| r.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("functionResponse"))
        .and_then(|fr| fr.get("response"));

    let mut result = response.cloned().unwrap_or_else(|| json!({}));
    if !result.is_object() {
        result = json!({ "output": result });
    }

    if let Some(obj) = result.as_object_mut() {
        if !result_text.is_empty() && !obj.contains_key("output") {
            obj.insert("output".to_string(), json!(result_text));
        }
        if let Some(agent_id) = tc.get("agentId") {
            obj.insert("agentId".to_string(), agent_id.clone());
        }
        if let Some(display) = tc.get("resultDisplay") {
            obj.insert("resultDisplay".to_string(), display.clone());
        }
    }

    if result.as_object().is_some_and(|obj| obj.is_empty()) {
        None
    } else {
        Some(result)
    }
}

fn gemini_tool_status(tc: &Value) -> Option<&str> {
    tc.get("status").and_then(|v| v.as_str())
}

fn gemini_tool_is_error(tc: &Value) -> Option<bool> {
    tc.get("isError")
        .and_then(|v| v.as_bool())
        .or_else(|| tc.get("error").map(|v| !v.is_null()))
        .or_else(|| gemini_tool_status(tc).map(|status| matches!(status, "error" | "failed")))
}

impl GeminiProvider {
    /// Parse a chat JSON file and return all sessions found (main + extracted subagent children).
    /// Returns empty vec if the file is a subagent file (kind == "subagent") or cannot be parsed.
    pub(super) fn parse_chat_file(
        &self,
        path: &PathBuf,
        project_id: &str,
        project_map: &HashMap<String, String>,
    ) -> Vec<ParsedSession> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let (chat, parse_warning_count) = match parse_chat_content(path, &content) {
            Some(parsed) => parsed,
            None => return Vec::new(),
        };

        let session_id = chat.session_id.clone();
        let is_sidechain = chat.kind.as_deref() == Some("subagent");
        let parent_id = if is_sidechain {
            gemini_parent_id_from_subagent_path(path)
        } else {
            None
        };

        let project_path = project_map
            .get(project_id)
            .cloned()
            .unwrap_or_else(|| NO_PROJECT.to_string());

        let project_name = project_name_from_path(&project_path);

        let mut messages = Vec::new();
        let mut first_user_message: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut model: Option<String> = None;

        for msg in &chat.messages {
            let role = match msg.msg_type.as_deref() {
                Some("user") => MessageRole::User,
                Some("model") | Some("gemini") | Some("assistant") => {
                    if model.is_none() {
                        if let Some(m) = &msg.model {
                            if !m.is_empty() {
                                model = Some(m.clone());
                            }
                        }
                    }
                    MessageRole::Assistant
                }
                Some("info") | Some("warning") | Some("error") => MessageRole::System,
                _ => continue,
            };

            // Prefer displayContent when Gemini recorded a UI-safe variant.
            let effective_content = if let Some(dc) = &msg.display_content {
                Some(dc.clone())
            } else {
                msg.content.clone()
            };

            // content can be a string or an array of {text, inlineData, fileData}
            let text = match &effective_content {
                Some(serde_json::Value::String(s)) => normalize_gemini_message(s, &project_path),
                Some(serde_json::Value::Array(arr)) => {
                    let has_binary_data = arr.iter().any(|item| {
                        item.get("inlineData").is_some() || item.get("fileData").is_some()
                    });

                    let mut parts = Vec::new();
                    for item in arr {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            let trimmed = text.trim();
                            if trimmed.starts_with("--- Content from referenced files ---")
                                || trimmed.starts_with("--- End of content ---")
                                || trimmed.is_empty()
                            {
                                continue;
                            }
                            let normalized = if has_binary_data {
                                strip_at_image_refs(trimmed)
                            } else {
                                normalize_gemini_message(trimmed, &project_path)
                            };
                            if !normalized.is_empty() {
                                parts.push(normalized);
                            }
                        } else if let Some(inline) = item.get("inlineData") {
                            let mime = inline
                                .get("mimeType")
                                .and_then(|m| m.as_str())
                                .unwrap_or("image/png");
                            if let Some(data) = inline.get("data").and_then(|d| d.as_str()) {
                                parts.push(format!("[Image: source: data:{mime};base64,{data}]"));
                            }
                        } else if let Some(file_data) = item.get("fileData") {
                            let mime = file_data
                                .get("mimeType")
                                .or_else(|| file_data.get("mimeData"))
                                .and_then(|m| m.as_str())
                                .unwrap_or("application/octet-stream");
                            let uri = file_data
                                .get("fileUri")
                                .or_else(|| file_data.get("uri"))
                                .or_else(|| file_data.get("name"))
                                .and_then(|u| u.as_str())
                                .unwrap_or("");
                            if uri.is_empty() {
                                continue;
                            }
                            let source = resolve_gemini_image_path(uri, &project_path)
                                .unwrap_or_else(|| uri.to_string());
                            if mime.starts_with("image/") || looks_like_image_path(&source) {
                                parts.push(format!("[Image: source: {source}]"));
                            } else {
                                parts.push(format!("[File: source: {source}, mime: {mime}]"));
                            }
                        }
                    }
                    parts.join("\n")
                }
                _ => String::new(),
            };

            let text = if role == MessageRole::System {
                match msg.msg_type.as_deref() {
                    Some("info" | "warning" | "error") => {
                        let label = msg.msg_type.as_deref().unwrap_or("info");
                        if text.is_empty() {
                            format!("[{label}]")
                        } else {
                            format!("[{label}]\n{text}")
                        }
                    }
                    _ => text,
                }
            } else {
                text
            };

            let has_thoughts = msg.thoughts.as_ref().is_some_and(|t| !t.is_empty());
            if text.is_empty() && msg.tool_calls.is_none() && !has_thoughts {
                continue;
            }

            let trimmed = text.trim_start();
            if !text.is_empty() && role != MessageRole::System && is_system_content(trimmed) {
                continue;
            }

            // Extract token usage for this turn
            let token_usage = msg.tokens.as_ref().and_then(|t| {
                let input = t.get("input").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let output = t.get("output").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let cached = t.get("cached").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if input == 0 && output == 0 {
                    None
                } else {
                    Some(TokenUsage {
                        input_tokens: input,
                        output_tokens: output,
                        cache_read_input_tokens: cached,
                        cache_creation_input_tokens: 0,
                    })
                }
            });

            // Emit thoughts as [thinking] system messages (before text)
            if role == MessageRole::Assistant {
                if let Some(ref thoughts) = msg.thoughts {
                    for thought in thoughts {
                        let subject = thought
                            .get("subject")
                            .and_then(|s| s.as_str())
                            .unwrap_or("");
                        let description = thought
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("");
                        if !description.is_empty() {
                            let thinking_ts = thought
                                .get("timestamp")
                                .and_then(|t| t.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| msg.timestamp.clone());
                            let content = if subject.is_empty() {
                                format!("[thinking]\n{description}")
                            } else {
                                format!("[thinking]\n**{subject}**\n{description}")
                            };
                            messages.push(Message {
                                role: MessageRole::System,
                                content,
                                timestamp: thinking_ts,
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                                model: None,
                                usage_hash: None,
                                tool_metadata: None,
                            });
                        }
                    }
                }
            }

            if !text.is_empty() {
                if role == MessageRole::User && first_user_message.is_none() {
                    first_user_message = Some(text.clone());
                }

                content_parts.push(text.clone());

                let has_tools = msg.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty());
                messages.push(Message {
                    role: role.clone(),
                    content: text,
                    timestamp: msg.timestamp.clone(),
                    tool_name: None,
                    tool_input: None,
                    token_usage: if !has_tools {
                        token_usage.clone()
                    } else {
                        None
                    },
                    model: if role == MessageRole::Assistant {
                        msg.model.clone()
                    } else {
                        None
                    },
                    usage_hash: None,
                    tool_metadata: None,
                });
            }

            // Extract tool calls as Tool messages
            if let Some(ref tool_calls) = msg.tool_calls {
                let last_idx = tool_calls.len().saturating_sub(1);
                for (i, tc) in tool_calls.iter().enumerate() {
                    let display_name = tc
                        .get("displayName")
                        .and_then(|n| n.as_str())
                        .or_else(|| tc.get("name").and_then(|n| n.as_str()))
                        .unwrap_or("tool");
                    let raw_name = tc
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or(display_name);
                    let metadata_input = tc.get("args");
                    let mut metadata = build_tool_metadata(ToolCallFacts {
                        provider: Provider::Gemini,
                        raw_name,
                        input: metadata_input,
                        call_id: tc
                            .get("id")
                            .or_else(|| tc.get("callId"))
                            .and_then(|v| v.as_str()),
                        assistant_id: msg.id.as_deref(),
                    });
                    let name = metadata.canonical_name.clone();

                    let is_agent = name == "Agent";

                    // Remap args for Bash: shell_command {command} or run_shell_command {command}
                    let args = match name.as_str() {
                        "Bash" => tc
                            .get("args")
                            .and_then(|a| {
                                let obj = a.as_object()?;
                                let cmd = obj
                                    .get("command")
                                    .or_else(|| obj.get("cmd"))
                                    .and_then(|c| c.as_str())?;
                                Some(serde_json::json!({"command": cmd}).to_string())
                            })
                            .or_else(|| tc.get("args").map(std::string::ToString::to_string)),
                        "Write" => tc
                            .get("args")
                            .and_then(|a| {
                                let obj = a.as_object()?;
                                let fp = obj.get("file_path").and_then(|f| f.as_str())?;
                                Some(serde_json::json!({"file_path": fp}).to_string())
                            })
                            .or_else(|| tc.get("args").map(std::string::ToString::to_string)),
                        _ => tc.get("args").map(std::string::ToString::to_string),
                    };

                    // Prefer resultDisplay (markdown string) over nested result extraction.
                    let result_text = tc
                        .get("resultDisplay")
                        .and_then(|rd| rd.as_str().map(String::from))
                        .or_else(|| {
                            tc.get("result")
                                .and_then(|r| r.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|item| item.get("functionResponse"))
                                .and_then(|fr| fr.get("response"))
                                .and_then(|resp| resp.get("output"))
                                .and_then(|o| o.as_str())
                                .map(String::from)
                        })
                        .unwrap_or_default();

                    // For Agent-type tools, prepend description to result content
                    let description = tc.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    let content = if !description.is_empty() && is_agent {
                        if result_text.is_empty() {
                            description.to_string()
                        } else {
                            format!("{description}\n\n{result_text}")
                        }
                    } else {
                        result_text.clone()
                    };

                    let result_value = gemini_tool_result_value(tc, &result_text);
                    enrich_tool_metadata(
                        &mut metadata,
                        ToolResultFacts {
                            raw_result: result_value.as_ref(),
                            is_error: gemini_tool_is_error(tc),
                            status: gemini_tool_status(tc),
                            artifact_path: None,
                        },
                    );

                    // Use tool-level timestamp if available, fall back to parent message
                    let tool_timestamp = tc
                        .get("timestamp")
                        .and_then(|t| t.as_str())
                        .map(String::from)
                        .or_else(|| msg.timestamp.clone());

                    messages.push(Message {
                        role: MessageRole::Tool,
                        content,
                        timestamp: tool_timestamp.clone(),
                        tool_name: Some(name.clone()),
                        tool_input: args,
                        token_usage: if i == last_idx {
                            token_usage.clone()
                        } else {
                            None
                        },
                        model: None,
                        usage_hash: None,
                        tool_metadata: Some(metadata),
                    });
                }
            }
        }

        if messages.is_empty() {
            return Vec::new();
        }

        let title_source = first_user_message.as_deref().or(chat.summary.as_deref());
        let title = session_title(title_source);
        let created_at = parse_rfc3339_timestamp(chat.start_time.as_deref());
        let updated_at = parse_rfc3339_timestamp(chat.last_updated.as_deref());
        let content_text = truncate_to_bytes(&content_parts.join("\n"), FTS_CONTENT_LIMIT);

        let meta = SessionMeta {
            id: session_id,
            provider: Provider::Gemini,
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
            model,
            cc_version: None,
            git_branch: None,
            parent_id,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };

        let main_session = ParsedSession {
            meta,
            messages,
            content_text,
            parse_warning_count,
        };

        vec![main_session]
    }
}

fn parse_chat_content(path: &Path, content: &str) -> Option<(ChatSession, u32)> {
    let ext = path.extension().and_then(|e| e.to_str());
    if ext == Some("jsonl") {
        parse_jsonl_chat(path, content)
    } else {
        match serde_json::from_str::<ChatSession>(content) {
            Ok(chat) => Some((chat, 0)),
            Err(error) => {
                log::warn!(
                    "failed to parse Gemini chat '{}': {}",
                    path.display(),
                    error
                );
                None
            }
        }
    }
}

fn parse_jsonl_chat(path: &Path, content: &str) -> Option<(ChatSession, u32)> {
    let mut metadata = JsonlMetadata::default();
    let mut messages: Vec<ChatMessage> = Vec::new();
    let mut message_indices: HashMap<String, usize> = HashMap::new();
    let mut parse_warning_count = 0_u32;

    for (line_index, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let record: Value = match serde_json::from_str(trimmed) {
            Ok(record) => record,
            Err(error) => {
                parse_warning_count = parse_warning_count.saturating_add(1);
                log::warn!(
                    "skipping malformed Gemini JSONL in '{}' at line {}: {}",
                    path.display(),
                    line_index + 1,
                    error
                );
                continue;
            }
        };

        if let Some(rewind_to) = record.get("$rewindTo").and_then(|v| v.as_str()) {
            if let Some(index) = message_indices.get(rewind_to).copied() {
                messages.truncate(index);
            } else {
                messages.clear();
            }
            rebuild_message_indices(&messages, &mut message_indices);
            continue;
        }

        metadata.apply(&record);

        if let Some(message_array) = record.get("messages").and_then(|v| v.as_array()) {
            for message_record in message_array {
                upsert_jsonl_message(
                    path,
                    line_index + 1,
                    message_record.clone(),
                    &mut messages,
                    &mut message_indices,
                    &mut parse_warning_count,
                );
            }
        } else if record.get("id").and_then(|v| v.as_str()).is_some() {
            upsert_jsonl_message(
                path,
                line_index + 1,
                record,
                &mut messages,
                &mut message_indices,
                &mut parse_warning_count,
            );
        }
    }

    let Some(session_id) = metadata.session_id else {
        log::warn!(
            "failed to parse Gemini JSONL '{}': missing sessionId metadata",
            path.display()
        );
        return None;
    };

    Some((
        ChatSession {
            session_id,
            start_time: metadata.start_time,
            last_updated: metadata.last_updated,
            kind: metadata.kind,
            summary: metadata.summary,
            messages,
        },
        parse_warning_count,
    ))
}

fn upsert_jsonl_message(
    path: &Path,
    line_number: usize,
    record: Value,
    messages: &mut Vec<ChatMessage>,
    message_indices: &mut HashMap<String, usize>,
    parse_warning_count: &mut u32,
) {
    let message: ChatMessage = match serde_json::from_value(record) {
        Ok(message) => message,
        Err(error) => {
            *parse_warning_count = parse_warning_count.saturating_add(1);
            log::warn!(
                "skipping malformed Gemini message in '{}' at line {}: {}",
                path.display(),
                line_number,
                error
            );
            return;
        }
    };

    if let Some(id) = message.id.as_ref() {
        if let Some(index) = message_indices.get(id).copied() {
            messages[index] = message;
        } else {
            message_indices.insert(id.clone(), messages.len());
            messages.push(message);
        }
    } else {
        messages.push(message);
    }
}

fn rebuild_message_indices(messages: &[ChatMessage], message_indices: &mut HashMap<String, usize>) {
    message_indices.clear();
    for (index, message) in messages.iter().enumerate() {
        if let Some(id) = message.id.as_ref() {
            message_indices.insert(id.clone(), index);
        }
    }
}
