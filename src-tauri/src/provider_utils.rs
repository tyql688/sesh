use std::path::Path;

use chrono::DateTime;

pub const NO_PROJECT: &str = "(No Project)";

pub fn is_system_content(trimmed: &str) -> bool {
    trimmed.starts_with("<environment_context")
        || trimmed.starts_with("<permissions")
        || trimmed.starts_with("<INSTRUCTIONS>")
        || trimmed.starts_with("<system")
        || trimmed.starts_with("<local-command-stdout>")
        || trimmed.starts_with("<local-command-caveat>")
        || trimmed.contains("<observation>")
        || trimmed.contains("</observation>")
        || trimmed.contains("<command-message>")
        || trimmed.contains("</command-message>")
        || trimmed.contains("</facts>")
        || trimmed.contains("</narrative>")
        || trimmed.contains("<INSTRUCTIONS>")
        || trimmed.contains("<environment_context>")
        || trimmed.contains("<permissions instructions>")
        || trimmed.contains("sandbox_mode")
        || (trimmed.starts_with('<') && trimmed.len() > 200 && !trimmed.contains("```"))
}

pub fn project_name_from_path(project_path: &str) -> String {
    if project_path.is_empty() || project_path == NO_PROJECT {
        NO_PROJECT.to_string()
    } else {
        Path::new(project_path)
            .file_name().map_or_else(|| project_path.to_string(), |name| name.to_string_lossy().to_string())
    }
}

pub fn parse_rfc3339_timestamp(timestamp: Option<&str>) -> i64 {
    timestamp
        .and_then(|ts| {
            DateTime::parse_from_rfc3339(ts)
                .map_err(|e| eprintln!("warn: failed to parse timestamp '{ts}': {e}"))
                .ok()
        })
        .map_or(0, |dt| dt.timestamp())
}

pub fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if input.chars().count() > max_chars {
        let mut truncated: String = input.chars().take(max_chars).collect();
        truncated.push_str("...");
        truncated
    } else {
        input.to_string()
    }
}

pub fn session_title(first_user_message: Option<&str>) -> String {
    first_user_message
        .map(|message| {
            // Strip [Image: source: ...] markers so titles show real text
            let cleaned = strip_image_markers(message);
            let text = cleaned.trim();
            if text.is_empty() {
                "Untitled".to_string()
            } else {
                truncate_with_ellipsis(text, 100)
            }
        })
        .unwrap_or_else(|| "Untitled".to_string())
}

fn strip_image_markers(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find("[Image") {
        result.push_str(&remaining[..start]);
        if let Some(end) = remaining[start..].find(']') {
            remaining = &remaining[start + end + 1..];
        } else {
            remaining = &remaining[start..];
            break;
        }
    }
    result.push_str(remaining);
    result
}

pub fn truncate_to_bytes(input: &str, max_bytes: usize) -> String {
    if input.len() > max_bytes {
        input[..input.floor_char_boundary(max_bytes)].to_string()
    } else {
        input.to_string()
    }
}
