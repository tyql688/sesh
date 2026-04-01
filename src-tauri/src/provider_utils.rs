use std::path::Path;

use chrono::DateTime;

pub const NO_PROJECT: &str = "(No Project)";
pub const FTS_CONTENT_LIMIT: usize = 2000;

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
        Path::new(project_path).file_name().map_or_else(
            || project_path.to_string(),
            |name| name.to_string_lossy().to_string(),
        )
    }
}

pub fn parse_rfc3339_timestamp(timestamp: Option<&str>) -> i64 {
    timestamp
        .and_then(|ts| {
            DateTime::parse_from_rfc3339(ts)
                .map_err(|e| log::warn!("failed to parse timestamp '{ts}': {e}"))
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

#[cfg(test)]
mod tests {
    use super::*;

    // --- project_name_from_path ---

    #[test]
    fn project_name_regular_path() {
        assert_eq!(
            project_name_from_path("/home/user/my-project"),
            "my-project"
        );
    }

    #[test]
    fn project_name_empty_string() {
        assert_eq!(project_name_from_path(""), NO_PROJECT);
    }

    #[test]
    fn project_name_no_project_sentinel() {
        assert_eq!(project_name_from_path(NO_PROJECT), NO_PROJECT);
    }

    #[test]
    fn project_name_root_path() {
        // Path::new("/").file_name() returns None, so falls through to project_path.to_string()
        let result = project_name_from_path("/");
        assert_eq!(result, "/");
    }

    // --- truncate_with_ellipsis ---

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string_with_ellipsis() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello...");
    }

    #[test]
    fn truncate_unicode_counts_chars_not_bytes() {
        let input = "你好世界测试"; // 6 chars, 18 bytes
        let result = truncate_with_ellipsis(input, 4);
        assert_eq!(result, "你好世界...");
    }

    // --- truncate_to_bytes ---

    #[test]
    fn truncate_bytes_ascii_within_limit() {
        assert_eq!(truncate_to_bytes("hello", 10), "hello");
    }

    #[test]
    fn truncate_bytes_ascii_beyond_limit() {
        let result = truncate_to_bytes("hello world", 5);
        assert_eq!(result, "hello");
    }

    #[test]
    fn truncate_bytes_multibyte_at_char_boundary() {
        let input = "你好世界"; // each char is 3 bytes = 12 bytes total
        let result = truncate_to_bytes(input, 7);
        // 7 bytes: floor_char_boundary → 6 bytes = 2 chars
        assert_eq!(result, "你好");
    }

    // --- session_title ---

    #[test]
    fn session_title_normal_message() {
        assert_eq!(session_title(Some("Fix the bug")), "Fix the bug");
    }

    #[test]
    fn session_title_none() {
        assert_eq!(session_title(None), "Untitled");
    }

    #[test]
    fn session_title_strips_image_markers() {
        let msg = "Hello [Image: source: /path/to/img.png] world";
        assert_eq!(session_title(Some(msg)), "Hello  world");
    }

    #[test]
    fn session_title_only_image_marker() {
        let msg = "[Image: source: data:image/png;base64,abc]";
        assert_eq!(session_title(Some(msg)), "Untitled");
    }

    #[test]
    fn session_title_long_message_truncated() {
        let msg = "a".repeat(200);
        let result = session_title(Some(&msg));
        assert_eq!(result.len(), 103); // 100 chars + "..."
        assert!(result.ends_with("..."));
    }

    // --- is_system_content ---

    #[test]
    fn is_system_environment_context() {
        assert!(is_system_content(
            "<environment_context>some stuff</environment_context>"
        ));
    }

    #[test]
    fn is_system_permissions() {
        assert!(is_system_content(
            "<permissions instructions>do things</permissions>"
        ));
    }

    #[test]
    fn is_system_normal_text_false() {
        assert!(!is_system_content("Just a normal message"));
    }

    #[test]
    fn is_system_short_unknown_tag_false() {
        assert!(!is_system_content("<short>"));
    }

    #[test]
    fn is_system_long_unknown_tag_true() {
        let long_tag = format!("<unknown_tag>{}</unknown_tag>", "x".repeat(250));
        assert!(is_system_content(&long_tag));
    }

    #[test]
    fn is_system_long_tag_with_code_block_false() {
        let content = format!("<unknown_tag>{}```code```</unknown_tag>", "x".repeat(250));
        assert!(!is_system_content(&content));
    }

    // --- parse_rfc3339_timestamp ---

    #[test]
    fn parse_valid_rfc3339() {
        let result = parse_rfc3339_timestamp(Some("2024-01-15T10:30:00Z"));
        assert!(result > 0);
    }

    #[test]
    fn parse_none_timestamp() {
        assert_eq!(parse_rfc3339_timestamp(None), 0);
    }

    #[test]
    fn parse_invalid_timestamp() {
        assert_eq!(parse_rfc3339_timestamp(Some("not-a-date")), 0);
    }
}
