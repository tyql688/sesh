use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT, NO_PROJECT,
};

use super::images::strip_at_image_refs;
use super::tools::{map_gemini_tool_name, normalize_gemini_message};
use super::{ChatSession, GeminiProvider};

impl GeminiProvider {
    pub(super) fn parse_chat_file(
        &self,
        path: &PathBuf,
        project_id: &str,
        project_map: &HashMap<String, String>,
    ) -> Option<ParsedSession> {
        let content = fs::read_to_string(path).ok()?;
        let file_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let chat: ChatSession = serde_json::from_str(&content).ok()?;

        let is_subagent = chat.kind.as_deref() == Some("subagent");

        // For subagent sessions, use filename stem as ID (unique per file);
        // for main/None, use chat.session_id as before.
        let session_id = if is_subagent {
            path.file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| chat.session_id.clone())
        } else {
            chat.session_id.clone()
        };

        // For subagent sessions, parent_id is the original session_id (shared with main session)
        let parent_id = if is_subagent {
            Some(chat.session_id.clone())
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
                    // Extract model from the first assistant message that has it
                    if model.is_none() {
                        if let Some(m) = &msg.model {
                            if !m.is_empty() {
                                model = Some(m.clone());
                            }
                        }
                    }
                    MessageRole::Assistant
                }
                Some("info") => MessageRole::System,
                _ => continue,
            };

            // For user messages, prefer displayContent over content when available
            let effective_content = if role == MessageRole::User {
                if let Some(dc) = &msg.display_content {
                    Some(dc.clone())
                } else {
                    msg.content.clone()
                }
            } else {
                msg.content.clone()
            };

            // content can be a string or an array of {text, inlineData}
            let text = match &effective_content {
                Some(serde_json::Value::String(s)) => normalize_gemini_message(s, &project_path),
                Some(serde_json::Value::Array(arr)) => {
                    // If inlineData exists, @path image refs in text are duplicates
                    let has_inline_data = arr.iter().any(|item| item.get("inlineData").is_some());

                    let mut parts = Vec::new();
                    for item in arr {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            // Filter Gemini context markers
                            let trimmed = text.trim();
                            if trimmed.starts_with("--- Content from referenced files ---")
                                || trimmed.starts_with("--- End of content ---")
                                || trimmed.is_empty()
                            {
                                continue;
                            }
                            let normalized = if has_inline_data {
                                // Strip @path image refs, keep only caption text
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
                        }
                    }
                    parts.join("\n")
                }
                _ => String::new(),
            };

            // For info messages, wrap content and don't skip empty
            let text = if role == MessageRole::System && msg.msg_type.as_deref() == Some("info") {
                if text.is_empty() {
                    "[info]".to_string()
                } else {
                    format!("[info]\n{text}")
                }
            } else {
                text
            };

            if text.is_empty() && msg.tool_calls.is_none() {
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
                    // Attach token usage to text msg only if no tool calls follow
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
                    let name = map_gemini_tool_name(display_name).to_string();

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

                    // Prefer resultDisplay (user-friendly markdown) over nested extraction
                    let result_text = tc
                        .get("resultDisplay")
                        .and_then(|rd| rd.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| {
                            tc.get("result")
                                .and_then(|r| r.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|item| item.get("functionResponse"))
                                .and_then(|fr| fr.get("response"))
                                .and_then(|resp| resp.get("output"))
                                .and_then(|o| o.as_str())
                                .unwrap_or("")
                                .to_string()
                        });

                    // For Agent-type tools, prepend description to result content
                    let description = tc
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let content = if !description.is_empty() && name == "Agent" {
                        if result_text.is_empty() {
                            description.to_string()
                        } else {
                            format!("{description}\n\n{result_text}")
                        }
                    } else {
                        result_text
                    };

                    // Use tool-level timestamp if available, fall back to parent message
                    let tool_timestamp = tc
                        .get("timestamp")
                        .and_then(|t| t.as_str())
                        .map(String::from)
                        .or_else(|| msg.timestamp.clone());

                    messages.push(Message {
                        role: MessageRole::Tool,
                        content,
                        timestamp: tool_timestamp,
                        tool_name: Some(name),
                        tool_input: args,
                        // Attach token usage to last tool message
                        token_usage: if i == last_idx {
                            token_usage.clone()
                        } else {
                            None
                        },
                        model: None,
                    });
                }
            }
        }

        if messages.is_empty() {
            return None;
        }

        let title = session_title(first_user_message.as_deref());

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
            is_sidechain: is_subagent,
            variant_name: None,
            model,
            cc_version: None,
            git_branch: None,
            parent_id,
        };

        Some(ParsedSession {
            meta,
            messages,
            content_text,
        })
    }
}
