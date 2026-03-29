use serde_json::Value;

/// Map Kimi CLI tool names to canonical display names.
pub fn map_kimi_tool_name(name: &str) -> &str {
    match name {
        "WriteFile" => "Write",
        "Shell" => "Bash",
        "ReadFile" => "Read",
        "StrReplaceFile" => "Edit",
        "Glob" => "Glob",
        "Grep" => "Grep",
        "Agent" => "Agent",
        _ => name,
    }
}

/// Extract readable output from a Kimi ToolResult payload.
pub fn extract_tool_output(payload: &Value) -> String {
    if let Some(rv) = payload.get("return_value") {
        // Prefer "message" field for concise output, fall back to "output"
        if let Some(msg) = rv.get("message").and_then(|v| v.as_str()) {
            if !msg.is_empty() {
                return msg.to_string();
            }
        }
        if let Some(out) = rv.get("output").and_then(|v| v.as_str()) {
            if !out.is_empty() {
                return out.to_string();
            }
        }
        // Try display array
        if let Some(Value::Array(arr)) = rv.get("display") {
            let texts: Vec<&str> = arr
                .iter()
                .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                .collect();
            if !texts.is_empty() {
                return texts.join("\n");
            }
        }
        // Fallback: serialize return_value
        return serde_json::to_string(rv).unwrap_or_default();
    }
    String::new()
}
