use serde_json::Value;

pub fn extract_codex_content(payload: &Value) -> String {
    match payload.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => extract_codex_array_content(arr),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => {
            // Also check for direct "output" field (function_call_output)
            payload
                .get("output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        }
    }
}

pub fn extract_codex_array_content(arr: &[Value]) -> String {
    let mut parts = Vec::new();

    for item in arr {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match item_type {
            "input_image" => {
                if let Some(image_url) = item.get("image_url").and_then(|v| v.as_str()) {
                    parts.push(format!("[Image: source: {image_url}]"));
                }
            }
            _ => {
                let Some(text) = extract_codex_text(item) else {
                    continue;
                };

                if is_codex_image_wrapper(text) {
                    continue;
                }

                parts.push(text.to_string());
            }
        }
    }

    parts.join("\n")
}

pub fn extract_codex_text(item: &Value) -> Option<&str> {
    item.get("text")
        .or_else(|| item.get("output_text"))
        .or_else(|| item.get("input_text"))
        .and_then(|t| t.as_str())
}

pub fn is_codex_image_wrapper(text: &str) -> bool {
    let trimmed = text.trim();
    (trimmed.starts_with("<image name=") && trimmed.ends_with('>')) || trimmed == "</image>"
}

/// Extract readable text from Codex tool output.
/// Handles: plain text, JSON `{"output":"..."}`, JSON array `[{"type":"text","text":"..."}]`.
pub fn extract_tool_output(raw: &str) -> String {
    let trimmed = raw.trim();
    // Try JSON object with "output" field (custom_tool_call_output)
    if trimmed.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            if let Some(out) = v.get("output").and_then(|o| o.as_str()) {
                return out.to_string();
            }
        }
    }
    // Try JSON array of text parts (MCP tool output)
    if trimmed.starts_with('[') {
        if let Ok(arr) = serde_json::from_str::<Vec<Value>>(trimmed) {
            let texts: Vec<&str> = arr
                .iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
    }
    raw.to_string()
}

/// Map Codex function names to display names matching our UI conventions.
pub fn map_codex_tool_name(name: &str) -> &str {
    match name {
        "exec_command" => "Bash",
        "apply_patch" => "Apply_patch",
        "view_image" => "Image",
        "update_plan" => "Plan",
        "spawn_agent" | "wait_agent" | "close_agent" => "Agent",
        "write_stdin" => "Stdin",
        _ if name.starts_with("mcp__") => {
            // e.g. mcp__playwright__browser_click -> last segment
            name.rsplit("__").next().unwrap_or(name)
        }
        _ => name,
    }
}

pub fn strip_inline_image_sources(text: &str) -> String {
    if !text.contains("[Image: source:") {
        return text.to_string();
    }

    text.lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("[Image: source:") {
                "[Image]".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
