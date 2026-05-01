use serde_json::Value;

/// Extract readable output from a Kimi ToolResult payload.
///
/// For shell tools (`Bash`/`Shell`), the raw `output` is preferred over generic
/// success messages like "Command executed successfully." so that the user sees
/// the actual command output.
pub fn extract_tool_output(payload: &Value, tool_name: Option<&str>) -> String {
    if let Some(rv) = payload.get("return_value") {
        // 1. Prefer structured display array (diffs, text blocks) when present.
        //    This covers Edit/StrReplaceFile tools that emit diff displays.
        if let Some(Value::Array(arr)) = rv.get("display") {
            if !arr.is_empty() {
                let texts: Vec<&str> = arr
                    .iter()
                    .filter(|item| item.get("type").and_then(|v| v.as_str()) != Some("diff"))
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect();
                if !texts.is_empty() {
                    return texts.join("\n");
                }

                let diffs: Vec<String> = arr
                    .iter()
                    .filter(|item| item.get("type").and_then(|v| v.as_str()) == Some("diff"))
                    .filter_map(|item| {
                        let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                        let old_text = item.get("old_text").and_then(|v| v.as_str()).unwrap_or("");
                        let new_text = item.get("new_text").and_then(|v| v.as_str()).unwrap_or("");
                        if old_text.is_empty() && new_text.is_empty() {
                            return None;
                        }
                        Some(format!("--- {}\n{}\n+++ {}\n{}", path, old_text, path, new_text))
                    })
                    .collect();
                if !diffs.is_empty() {
                    return diffs.join("\n\n");
                }
            }
        }

        // 2. Shell tools: prefer raw output over generic success messages
        let prefer_output = matches!(tool_name, Some("Bash") | Some("Shell"));

        if !prefer_output {
            if let Some(msg) = rv.get("message").and_then(|v| v.as_str()) {
                if !msg.is_empty() {
                    return msg.to_string();
                }
            }
        }

        if let Some(out) = rv.get("output").and_then(|v| v.as_str()) {
            if !out.is_empty() {
                return out.to_string();
            }
        }

        if prefer_output {
            if let Some(msg) = rv.get("message").and_then(|v| v.as_str()) {
                if !msg.is_empty() {
                    return msg.to_string();
                }
            }
        }

        // Fallback: serialize return_value
        return serde_json::to_string(rv).unwrap_or_default();
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_prefers_output_over_generic_message() {
        let payload = serde_json::json!({
            "return_value": {
                "is_error": false,
                "output": "total 16\ndrwxr-xr-x 2 user user 4096 Jan  1 10:00 .",
                "message": "Command executed successfully."
            }
        });
        assert_eq!(
            extract_tool_output(&payload, Some("Bash")),
            "total 16\ndrwxr-xr-x 2 user user 4096 Jan  1 10:00 ."
        );
    }

    #[test]
    fn non_shell_prefers_message_over_output() {
        let payload = serde_json::json!({
            "return_value": {
                "is_error": false,
                "output": "     1\tdefault_model =...",
                "message": "11 lines read from file starting from line 1."
            }
        });
        assert_eq!(
            extract_tool_output(&payload, Some("Read")),
            "11 lines read from file starting from line 1."
        );
    }

    #[test]
    fn shell_falls_back_to_message_when_output_empty() {
        let payload = serde_json::json!({
            "return_value": {
                "is_error": false,
                "output": "",
                "message": "File successfully edited."
            }
        });
        assert_eq!(
            extract_tool_output(&payload, Some("Shell")),
            "File successfully edited."
        );
    }

    #[test]
    fn extracts_diff_display() {
        let payload = serde_json::json!({
            "return_value": {
                "is_error": false,
                "output": "",
                "message": "File successfully edited.",
                "display": [
                    {
                        "type": "diff",
                        "path": "/tmp/config.toml",
                        "old_text": "default_yolo = false",
                        "new_text": "default_yolo = true",
                        "old_start": 1,
                        "new_start": 1,
                        "is_summary": false
                    }
                ]
            }
        });
        let out = extract_tool_output(&payload, Some("Edit"));
        assert!(out.contains("--- /tmp/config.toml"));
        assert!(out.contains("default_yolo = false"));
        assert!(out.contains("+++ /tmp/config.toml"));
        assert!(out.contains("default_yolo = true"));
    }

    #[test]
    fn text_display_takes_precedence_over_diff() {
        let payload = serde_json::json!({
            "return_value": {
                "display": [
                    { "type": "text", "text": "Summary line" },
                    { "type": "diff", "path": "/tmp/x", "old_text": "a", "new_text": "b" }
                ]
            }
        });
        assert_eq!(extract_tool_output(&payload, None), "Summary line");
    }
}
