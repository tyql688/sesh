use serde_json::Value;

use crate::provider_utils::is_system_content;

/// Extract clean user text: strip <user_query> tags, filter system content.
pub fn extract_user_text(text: &str) -> String {
    // Try extracting from <user_query> tags
    if let Some(inner) = extract_tag_content(text, "user_query") {
        let trimmed = inner.trim();
        if !trimmed.is_empty() && !is_system_content(trimmed) {
            return trimmed.to_string();
        }
    }
    // If no user_query tag, check if it's system content
    let trimmed = text.trim();
    if trimmed.is_empty()
        || is_system_content(trimmed)
        || trimmed.starts_with("<user_info>")
        || trimmed.starts_with("<agent_transcripts>")
    {
        return String::new();
    }
    trimmed.to_string()
}

/// Extract content between <tag>...</tag>.
pub fn extract_tag_content<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)?;
    let after = &text[start + open.len()..];
    let end = after.find(&close)?;
    Some(&after[..end])
}

/// Extract workspace path from user_info XML in user messages.
pub fn extract_workspace_path(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Workspace Path:") {
            let path = rest.trim();
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

pub fn extract_text_from_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => {
            // Content might be a JSON array serialized as string
            if s.trim_start().starts_with('[') {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(s) {
                    return extract_text_from_parts(&arr);
                }
            }
            s.clone()
        }
        Some(Value::Array(arr)) => extract_text_from_parts(arr),
        _ => String::new(),
    }
}

pub fn extract_text_from_parts(arr: &[Value]) -> String {
    arr.iter()
        .filter_map(|item| {
            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match item_type {
                "text" => {
                    let text = item.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    // Filter out redacted reasoning placeholders
                    if text.trim() == "[REDACTED]" {
                        None
                    } else {
                        Some(text.to_string())
                    }
                }
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse content field as array of Value parts.
pub fn parse_content_array(content: Option<&Value>) -> Vec<Value> {
    match content {
        Some(Value::Array(arr)) => arr.clone(),
        Some(Value::String(s)) => {
            if s.trim_start().starts_with('[') {
                serde_json::from_str::<Vec<Value>>(s).unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

/// Strip <think>...</think> tags, return remaining visible text.
pub fn strip_think_tags(text: &str) -> String {
    if !text.contains("<think>") {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;
    while let Some(start_idx) = remaining.find("<think>") {
        result.push_str(&remaining[..start_idx]);
        remaining = &remaining[start_idx + 7..]; // skip "<think>"
        if let Some(end_idx) = remaining.find("</think>") {
            remaining = &remaining[end_idx + 8..]; // skip "</think>"
        } else {
            break; // unclosed tag, stop
        }
    }
    result.push_str(remaining);
    result.trim().to_string()
}

/// Extract <think>...</think> content.
pub fn extract_think_content(text: &str) -> Option<String> {
    let start = text.find("<think>")?;
    let after = &text[start + "<think>".len()..];
    let end = after.find("</think>").unwrap_or(after.len());
    let thinking = after[..end].trim();
    if thinking.is_empty() {
        None
    } else {
        Some(thinking.to_string())
    }
}

/// Strip Cursor-style bold-heading thinking blocks from assistant text.
///
/// Cursor models emit internal reasoning as `**Bold Title**\n\nParagraph...`
/// blocks appended to (or replacing) the visible response. This function
/// removes them and returns only the user-facing text.
pub fn strip_cursor_thinking(text: &str) -> String {
    if !text.contains("**") {
        return text.to_string();
    }
    let (visible, _) = split_cursor_thinking(text);
    visible
}

/// Extract Cursor-style bold-heading thinking blocks as a single string.
pub fn extract_cursor_thinking(text: &str) -> Option<String> {
    if !text.contains("**") {
        return None;
    }
    let (_, thinking) = split_cursor_thinking(text);
    if thinking.is_empty() {
        None
    } else {
        Some(thinking)
    }
}

/// Split text into (visible, thinking) based on `**Title**\n` patterns.
///
/// A thinking block starts at `\n**` (or at start-of-string `**`) where
/// the bold title is closed on the same line, followed by a newline and
/// English-language reasoning paragraphs.
fn split_cursor_thinking(text: &str) -> (String, String) {
    let mut visible = String::new();
    let mut thinking = String::new();

    // Find the first bold-heading block boundary
    let first_bold = find_bold_heading_start(text);

    match first_bold {
        Some(pos) => {
            let before = text[..pos].trim_end();
            if !before.is_empty() {
                visible.push_str(before);
            }
            // Everything from the first bold heading onward is thinking
            let rest = text[pos..].trim();
            // Strip the ** markers from thinking for cleaner display
            for line in rest.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("**") && trimmed.ends_with("**") && trimmed.len() > 4 {
                    // Bold title line → keep as section header without **
                    if !thinking.is_empty() {
                        thinking.push('\n');
                    }
                    thinking.push_str(&trimmed[2..trimmed.len() - 2]);
                    thinking.push('\n');
                } else {
                    thinking.push_str(line);
                    thinking.push('\n');
                }
            }
        }
        None => {
            visible.push_str(text);
        }
    }

    (visible.trim().to_string(), thinking.trim().to_string())
}

/// Find the byte offset of the first `**Title**\n` pattern that looks like
/// a Cursor thinking block (bold heading followed by reasoning paragraph).
fn find_bold_heading_start(text: &str) -> Option<usize> {
    // Pattern 1: starts at beginning of text
    if text.starts_with("**") && is_bold_heading_at(text, 0) {
        return Some(0);
    }

    // Pattern 2: after \n\n or \n
    let mut search_from = 0;
    while let Some(nl_pos) = text[search_from..].find('\n') {
        let abs_pos = search_from + nl_pos;
        let after_nl = abs_pos + 1;
        if after_nl >= text.len() {
            break;
        }
        // Skip additional newlines
        let mut bold_start = after_nl;
        while bold_start < text.len() && text.as_bytes()[bold_start] == b'\n' {
            bold_start += 1;
        }
        if bold_start < text.len()
            && text[bold_start..].starts_with("**")
            && is_bold_heading_at(text, bold_start)
        {
            return Some(after_nl);
        }
        search_from = after_nl;
    }

    None
}

/// Check if position `pos` in `text` starts a `**Title**\n` bold heading.
fn is_bold_heading_at(text: &str, pos: usize) -> bool {
    let rest = &text[pos..];
    if !rest.starts_with("**") {
        return false;
    }
    let after_open = &rest[2..];
    // Find closing ** on the same line
    if let Some(close) = after_open.find("**") {
        let title = &after_open[..close];
        // Title should be short (< 80 chars), single line, no nested **
        if title.len() > 80 || title.contains('\n') {
            return false;
        }
        // After the closing **, there should be a newline (end of heading)
        let after_close = &after_open[close + 2..];
        if after_close.is_empty() || after_close.starts_with('\n') {
            return true;
        }
    }
    false
}

/// Strip `[REDACTED]` placeholders from text content.
pub fn strip_redacted(text: &str) -> String {
    let result = text.replace("[REDACTED]", "");
    // Clean up leftover blank lines
    let lines: Vec<&str> = result.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.join("\n")
}

/// Remap tool args to match canonical format for frontend display.
pub fn remap_tool_args(tool_name: &str, args: &Value) -> Option<String> {
    // ApplyPatch input is raw patch text (a string), not a JSON object.
    // Extract the file path from the patch header and pass content through.
    if let Value::String(s) = args {
        return Some(remap_patch_string(s));
    }

    let obj = args.as_object()?;
    match tool_name {
        "Bash" => {
            let cmd = obj
                .get("command")
                .or_else(|| obj.get("input"))
                .and_then(|c| c.as_str())?;
            Some(serde_json::json!({"command": cmd}).to_string())
        }
        "Write" => {
            let path = obj
                .get("path")
                .or_else(|| obj.get("file_path"))
                .and_then(|p| p.as_str())?;
            Some(serde_json::json!({"file_path": path}).to_string())
        }
        "Read" => {
            let path = obj
                .get("path")
                .or_else(|| obj.get("file_path"))
                .and_then(|p| p.as_str())?;
            Some(serde_json::json!({"file_path": path}).to_string())
        }
        "Edit" => {
            let path = obj
                .get("path")
                .or_else(|| obj.get("file_path"))
                .and_then(|p| p.as_str())
                .unwrap_or("");
            let old = obj
                .get("old_str")
                .or_else(|| obj.get("old_string"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            let new = obj
                .get("new_str")
                .or_else(|| obj.get("new_string"))
                .and_then(|s| s.as_str())
                .unwrap_or("");
            Some(
                serde_json::json!({"file_path": path, "old_string": old, "new_string": new})
                    .to_string(),
            )
        }
        "Glob" => {
            let pattern = obj.get("pattern").and_then(|p| p.as_str())?;
            Some(serde_json::json!({"pattern": pattern}).to_string())
        }
        "Grep" => {
            let pattern = obj.get("pattern").and_then(|p| p.as_str())?;
            let path = obj.get("path").and_then(|p| p.as_str());
            let mut j = serde_json::json!({"pattern": pattern});
            if let Some(p) = path {
                j["path"] = serde_json::json!(p);
            }
            Some(j.to_string())
        }
        _ => Some(args.to_string()),
    }
}

/// Extract file path from a raw ApplyPatch string and format for display.
/// Patch format: `*** Begin Patch\n*** Update File: /path\n@@\n-old\n+new\n*** End Patch`
fn remap_patch_string(patch: &str) -> String {
    let mut file_path = "";
    for line in patch.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("*** Update File: ")
            .or_else(|| trimmed.strip_prefix("*** Add File: "))
            .or_else(|| trimmed.strip_prefix("*** Delete File: "))
        {
            file_path = rest.trim();
            break;
        }
    }
    serde_json::json!({"file_path": file_path, "patch": patch}).to_string()
}
