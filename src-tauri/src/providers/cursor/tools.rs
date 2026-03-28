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

/// Extract text from message content (string or array of parts).
pub fn extract_text_content(msg: &Value) -> String {
    extract_text_from_content(msg.get("content"))
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
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
            } else {
                None
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
    let mut result = text.to_string();
    while let Some(start) = result.find("<think>") {
        if let Some(end) = result.find("</think>") {
            result = format!("{}{}", &result[..start], &result[end + "</think>".len()..]);
        } else {
            result = result[..start].to_string();
            break;
        }
    }
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

/// Map Cursor tool names to canonical display names.
pub fn map_cursor_tool_name(name: &str) -> &str {
    match name {
        "Shell" | "shell" => "Bash",
        "Write" | "write" => "Write",
        "Read" | "read" => "Read",
        "StrReplace" | "str_replace" => "Edit",
        "Glob" | "glob" => "Glob",
        "Grep" | "grep" => "Grep",
        "Delete" | "delete" => "Delete",
        "ReadLints" => "Lint",
        _ => name,
    }
}

/// Remap tool args to match canonical format for frontend display.
pub fn remap_tool_args(tool_name: &str, args: &Value) -> Option<String> {
    let obj = args.as_object()?;
    match tool_name {
        "Bash" => {
            let cmd = obj.get("command").or_else(|| obj.get("input")).and_then(|c| c.as_str())?;
            Some(serde_json::json!({"command": cmd}).to_string())
        }
        "Write" => {
            let path = obj.get("path").or_else(|| obj.get("file_path")).and_then(|p| p.as_str())?;
            Some(serde_json::json!({"file_path": path}).to_string())
        }
        "Read" => {
            let path = obj.get("path").or_else(|| obj.get("file_path")).and_then(|p| p.as_str())?;
            Some(serde_json::json!({"file_path": path}).to_string())
        }
        "Edit" => {
            let path = obj.get("path").or_else(|| obj.get("file_path")).and_then(|p| p.as_str()).unwrap_or("");
            let old = obj.get("old_str").or_else(|| obj.get("old_string")).and_then(|s| s.as_str()).unwrap_or("");
            let new = obj.get("new_str").or_else(|| obj.get("new_string")).and_then(|s| s.as_str()).unwrap_or("");
            Some(serde_json::json!({"file_path": path, "old_string": old, "new_string": new}).to_string())
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
