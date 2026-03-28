use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use rayon::prelude::*;
use serde_json::Value;

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes,
};

pub struct ClaudeProvider {
    home_dir: PathBuf,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().expect("cannot resolve HOME directory — app cannot function without it");
        Self { home_dir }
    }

    fn projects_dir(&self) -> PathBuf {
        self.home_dir.join(".claude").join("projects")
    }

    fn collect_jsonl_files(&self) -> Vec<PathBuf> {
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return Vec::new();
        }
        let mut all_files: Vec<PathBuf> = Vec::new();
        let project_dirs = match fs::read_dir(&projects_dir) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("warn: cannot read Claude projects dir '{}': {}", projects_dir.display(), e);
                return Vec::new();
            }
        };
        for entry in project_dirs {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let project_dir = entry.path();
            if !project_dir.is_dir() {
                continue;
            }
            let files = match fs::read_dir(&project_dir) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for file_entry in files {
                let file_entry = match file_entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let file_path = file_entry.path();
                if file_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    all_files.push(file_path);
                }
            }
        }
        all_files
    }

    fn parse_session(&self, path: &PathBuf) -> Option<ParsedSession> {
        let file = File::open(path).ok()?;
        let metadata = fs::metadata(path).ok()?;
        let file_size = metadata.len();

        let reader = BufReader::new(file);
        let mut messages = Vec::new();
        let mut first_user_message: Option<String> = None;
        let mut summary_text: Option<String> = None;
        let mut first_timestamp: Option<String> = None;
        let mut last_timestamp: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut cwd: Option<String> = None;
        let mut pending_user_message: Option<(String, Option<String>)> = None;
        let mut is_sidechain = false;
        // Map tool_use_id → index in messages vec, for merging tool_result back
        let mut tool_use_id_map: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

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
                    eprintln!("warn: skipping malformed JSONL in '{}': {}", path.display(), e);
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
            if !is_sidechain {
                if entry.get("isSidechain").and_then(|v| v.as_bool()).unwrap_or(false) {
                    is_sidechain = true;
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
                    let msg = match entry.get("message") {
                        Some(m) => m,
                        None => continue,
                    };

                    // Check if this "user" entry is actually a tool_result
                    // (the Anthropic API sends tool results as user-role turns)
                    if is_tool_result_message(msg) {
                        flush_pending_user_message(
                            &mut pending_user_message,
                            &mut messages,
                            &mut content_parts,
                            &mut first_user_message,
                        );
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
                                content_parts.push(result_text.clone());
                                let use_id = result_item.get("tool_use_id").and_then(|i| i.as_str());
                                if let Some(idx) = use_id.and_then(|id| tool_use_id_map.get(id)) {
                                    // Merge result into the existing tool_use message
                                    messages[*idx].content = result_text;
                                } else {
                                    // No matching tool_use found — emit as standalone
                                    messages.push(Message {
                                        role: MessageRole::Tool,
                                        content: result_text,
                                        timestamp: timestamp.clone(),
                                        tool_name: use_id.map(std::string::ToString::to_string),
                                        tool_input: None,
                                        token_usage: None,
                                    });
                                }
                            }
                        }
                        continue;
                    }

                    let text = extract_message_content(msg);
                    if text.trim().is_empty() {
                        continue;
                    }
                    let is_meta = entry
                        .get("isMeta")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);

                    if let Some((pending_text, pending_timestamp)) = pending_user_message.take() {
                        if is_meta
                            && contains_image_placeholder_without_source(&pending_text)
                            && contains_image_source(&text)
                        {
                            append_user_message(
                                &mut messages,
                                &mut content_parts,
                                &mut first_user_message,
                                merge_image_placeholders_with_sources(&pending_text, &text),
                                pending_timestamp,
                            );
                            continue;
                        }

                        append_user_message(
                            &mut messages,
                            &mut content_parts,
                            &mut first_user_message,
                            pending_text,
                            pending_timestamp,
                        );
                    }

                    if contains_image_placeholder_without_source(&text) {
                        pending_user_message = Some((text, timestamp));
                        continue;
                    }

                    if !is_meta || contains_image_source(&text) {
                        append_user_message(
                            &mut messages,
                            &mut content_parts,
                            &mut first_user_message,
                            text,
                            timestamp,
                        );
                    }
                }
                "assistant" => {
                    flush_pending_user_message(
                        &mut pending_user_message,
                        &mut messages,
                        &mut content_parts,
                        &mut first_user_message,
                    );
                    let msg = match entry.get("message") {
                        Some(m) => m,
                        None => continue,
                    };

                    // Extract token usage for this assistant turn
                    let turn_usage = extract_token_usage(msg);
                    let turn_start = messages.len();

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
                                            messages.push(Message {
                                                role: MessageRole::System,
                                                content: format!("[thinking]\n{t}"),
                                                timestamp: timestamp.clone(),
                                                tool_name: None,
                                                tool_input: None,
                                                token_usage: None,
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
                                        content_parts.push(text.clone());
                                        messages.push(Message {
                                            role: MessageRole::Assistant,
                                            content: text,
                                            timestamp: timestamp.clone(),
                                            tool_name: None,
                                            tool_input: None,
                                            token_usage: None,
                                        });
                                        text_parts.clear();
                                    }
                                    // Emit tool_use as a Tool message
                                    let name =
                                        item.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                                    let input = item
                                        .get("input")
                                        .map(std::string::ToString::to_string);
                                    let msg_idx = messages.len();
                                    messages.push(Message {
                                        role: MessageRole::Tool,
                                        content: String::new(),
                                        timestamp: timestamp.clone(),
                                        tool_name: Some(name.to_string()),
                                        tool_input: input,
                                        token_usage: None,
                                    });
                                    // Record tool_use_id for merging results later
                                    if let Some(id) = item.get("id").and_then(|i| i.as_str()) {
                                        tool_use_id_map.insert(id.to_string(), msg_idx);
                                    }
                                }
                                _ => {}
                            }
                        }
                        // Flush remaining text
                        if !text_parts.is_empty() {
                            let text = text_parts.join("\n");
                            content_parts.push(text.clone());
                            messages.push(Message {
                                role: MessageRole::Assistant,
                                content: text,
                                timestamp,
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                            });
                        }
                    } else {
                        // content is a plain string
                        let text = extract_message_content(msg);
                        if !text.trim().is_empty() {
                            content_parts.push(text.clone());
                            messages.push(Message {
                                role: MessageRole::Assistant,
                                content: text,
                                timestamp,
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                            });
                        }
                    }

                    // Attach token usage to the last assistant/tool message of this turn
                    if let Some(usage) = turn_usage {
                        // Find the last non-thinking message in this turn
                        if let Some(last_msg) = messages[turn_start..].iter_mut()
                            .filter(|m| m.role != MessageRole::System)
                            .last()
                        {
                            last_msg.token_usage = Some(usage);
                        }
                    }
                }
                "summary" => {
                    if summary_text.is_none() {
                        if let Some(s) = entry.get("summary").and_then(|s| s.as_str()) {
                            if !s.trim().is_empty() {
                                summary_text = Some(s.to_string());
                            }
                        }
                    }
                    flush_pending_user_message(
                        &mut pending_user_message,
                        &mut messages,
                        &mut content_parts,
                        &mut first_user_message,
                    );
                    continue;
                }
                // Skip all other types
                _ => {
                    flush_pending_user_message(
                        &mut pending_user_message,
                        &mut messages,
                        &mut content_parts,
                        &mut first_user_message,
                    );
                    continue;
                }
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

        let session_id = path.file_stem()?.to_string_lossy().to_string();

        let project_path = cwd.unwrap_or_default();
        let project_name = project_name_from_path(&project_path);

        let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

        let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

        let full_content = content_parts.join("\n");
        let content_text = truncate_to_bytes(&full_content, 2000);

        let title = session_title(first_user_message.as_deref().or(summary_text.as_deref()));

        let meta = SessionMeta {
            id: session_id,
            provider: Provider::Claude,
            title,
            project_path,
            project_name,
            created_at,
            updated_at,
            message_count: messages.len() as u32,
            file_size_bytes: file_size,
            source_path: path.to_string_lossy().to_string(),
            is_sidechain,
        };

        Some(ParsedSession {
            meta,
            messages,
            content_text,
        })
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
    });
}

fn flush_pending_user_message(
    pending_user_message: &mut Option<(String, Option<String>)>,
    messages: &mut Vec<Message>,
    content_parts: &mut Vec<String>,
    first_user_message: &mut Option<String>,
) {
    if let Some((text, timestamp)) = pending_user_message.take() {
        append_user_message(messages, content_parts, first_user_message, text, timestamp);
    }
}

/// Extract token usage from a message's `usage` field.
fn extract_token_usage(message: &Value) -> Option<TokenUsage> {
    let usage = message.get("usage")?;
    let input_tokens = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let output_tokens = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
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
                        let input = item.get("input").map(std::string::ToString::to_string).unwrap_or_default();
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
                            parts.push(format!(
                                "[Image: source: data:{};base64,{}]",
                                media, b64
                            ));
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

fn contains_image_source(text: &str) -> bool {
    text.contains("[Image: source:")
}

fn contains_image_placeholder_without_source(text: &str) -> bool {
    text.contains("[Image") && !contains_image_source(text)
}

fn merge_image_placeholders_with_sources(placeholder_text: &str, meta_text: &str) -> String {
    let sources = extract_image_source_segments(meta_text);
    if sources.is_empty() {
        return placeholder_text.to_string();
    }

    let mut merged = String::new();
    let mut remaining = placeholder_text;
    let mut source_index = 0usize;

    while let Some(start) = remaining.find("[Image") {
        merged.push_str(&remaining[..start]);
        let image_slice = &remaining[start..];
        let Some(end_offset) = image_slice.find(']') else {
            merged.push_str(image_slice);
            remaining = "";
            break;
        };

        let candidate = &image_slice[..=end_offset];
        if source_index < sources.len() && is_image_placeholder(candidate) {
            merged.push_str(&sources[source_index]);
            source_index += 1;
        } else {
            merged.push_str(candidate);
        }

        remaining = &image_slice[end_offset + 1..];
    }

    merged.push_str(remaining);

    if source_index < sources.len() {
        if !merged.is_empty() && !merged.ends_with('\n') {
            merged.push('\n');
        }
        merged.push_str(&sources[source_index..].join("\n"));
    }

    merged
}

fn extract_image_source_segments(text: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut remaining = text;

    while let Some(start) = remaining.find("[Image") {
        let image_slice = &remaining[start..];
        let Some(end_offset) = image_slice.find(']') else {
            break;
        };

        let candidate = &image_slice[..=end_offset];
        if contains_image_source(candidate) {
            segments.push(candidate.to_string());
        }

        remaining = &image_slice[end_offset + 1..];
    }

    segments
}

fn is_image_placeholder(segment: &str) -> bool {
    segment.starts_with("[Image") && !segment.contains("source:")
}

impl SessionProvider for ClaudeProvider {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.projects_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let all_files = self.collect_jsonl_files();

        let sessions: Vec<ParsedSession> = all_files
            .par_iter()
            .filter_map(|path| self.parse_session(path))
            .collect();

        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        Ok(self.parse_session(&path).into_iter().collect())
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);

        let parsed = self
            .parse_session(&path)
            .ok_or_else(|| ProviderError::Parse("failed to parse session file".to_string()))?;

        Ok(parsed.messages)
    }
}

#[cfg(test)]
mod tests {
    use super::merge_image_placeholders_with_sources;

    #[test]
    fn merge_image_placeholders_preserves_caption_text() {
        let placeholder = "[Image #1] 这个不展示了";
        let meta = "[Image: source: /tmp/1.png]";

        assert_eq!(
            merge_image_placeholders_with_sources(placeholder, meta),
            "[Image: source: /tmp/1.png] 这个不展示了"
        );
    }

    #[test]
    fn merge_image_placeholders_handles_multiple_images() {
        let placeholder = "[Image #1][Image #2] 小窗口不能适应？";
        let meta = "[Image: source: /tmp/1.png][Image: source: /tmp/2.png]";

        assert_eq!(
            merge_image_placeholders_with_sources(placeholder, meta),
            "[Image: source: /tmp/1.png][Image: source: /tmp/2.png] 小窗口不能适应？"
        );
    }
}
