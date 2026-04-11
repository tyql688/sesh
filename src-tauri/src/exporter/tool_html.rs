use std::path::Path;

use serde_json::Value;
use similar::{ChangeTag, TextDiff};

use crate::models::ToolMetadata;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn truncate_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn short_path(p: &str) -> String {
    let path = Path::new(p);
    path.iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|c| c.to_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("/")
}

fn string_field<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| obj.get(*key).and_then(|v| v.as_str()))
}

fn render_field(label: &str, value: &str) -> String {
    format!(
        r#"<div class="tool-field"><span class="tool-field-label">{}</span><span class="tool-field-value">{}</span></div>"#,
        html_escape(label),
        html_escape(value)
    )
}

fn render_pre_field(label: &str, value: &str) -> String {
    format!(
        r#"<div class="tool-field"><span class="tool-field-label">{}</span><pre class="tool-cmd">{}</pre></div>"#,
        html_escape(label),
        html_escape(value)
    )
}

fn render_diff_line(kind: &str, text: &str) -> String {
    let marker = match kind {
        "add" => "+",
        "remove" => "-",
        "skip" => "⋯",
        _ => " ",
    };
    format!(
        r#"<div class="tool-diff-line {kind}"><span class="tool-diff-gutter"></span><span class="tool-diff-gutter"></span><span class="tool-diff-marker">{marker}</span><span class="tool-diff-code">{}</span></div>"#,
        html_escape(if text.is_empty() { " " } else { text })
    )
}

pub(crate) fn render_line_diff(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut html = String::from(r#"<div class="tool-line-diff">"#);

    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Delete => "remove",
            ChangeTag::Insert => "add",
            ChangeTag::Equal => "context",
        };
        html.push_str(&render_diff_line(
            kind,
            change.value().trim_end_matches('\n'),
        ));
    }

    html.push_str("</div>");
    html
}

pub(crate) fn render_patch_diff(patch: &str) -> String {
    let mut html = String::from(r#"<div class="tool-line-diff">"#);

    for line in patch.lines() {
        if line == "*** Begin Patch" || line == "*** End Patch" || line.is_empty() {
            continue;
        }
        if line.starts_with("*** ") || line.starts_with("@@") {
            html.push_str(&render_diff_line("skip", line));
        } else if let Some(rest) = line.strip_prefix('+') {
            html.push_str(&render_diff_line("add", rest));
        } else if let Some(rest) = line.strip_prefix('-') {
            html.push_str(&render_diff_line("remove", rest));
        } else if let Some(rest) = line.strip_prefix(' ') {
            html.push_str(&render_diff_line("context", rest));
        } else {
            html.push_str(&render_diff_line("skip", line));
        }
    }

    html.push_str("</div>");
    html
}

pub(crate) fn tool_icon(name: &str, metadata: Option<&ToolMetadata>) -> &'static str {
    if metadata.is_some_and(|m| m.category == "mcp") || name.starts_with("mcp__") {
        return "🔌";
    }
    match metadata.map(|m| m.canonical_name.as_str()).unwrap_or(name) {
        "Read" => "📄",
        "Edit" | "Apply_patch" => "✏️",
        "Write" => "📝",
        "Bash" => "💻",
        "Glob" => "🔍",
        "Grep" => "🔎",
        "Agent" => "🤖",
        "Plan" | "TaskCreate" | "TaskUpdate" | "TaskList" => "📋",
        "TaskStop" => "🛑",
        "WebSearch" | "WebFetch" => "🌐",
        "ToolSearch" => "🧰",
        "Skill" => "⚡",
        "AskUserQuestion" => "❓",
        _ => "⚙",
    }
}

pub(crate) fn tool_display_name<'a>(name: &'a str, metadata: Option<&'a ToolMetadata>) -> &'a str {
    metadata.map(|m| m.display_name.as_str()).unwrap_or(name)
}

pub(crate) fn tool_summary(name: &str, input: &str, metadata: Option<&ToolMetadata>) -> String {
    if let Some(summary) = metadata.and_then(|m| m.summary.as_deref()) {
        return summary.to_string();
    }
    if name == "Apply_patch" || (name == "Edit" && input.contains("*** Begin Patch")) {
        return input
            .lines()
            .find(|l| {
                l.starts_with("*** Add File:")
                    || l.starts_with("*** Update File:")
                    || l.starts_with("*** Delete File:")
            })
            .and_then(|l| l.split(':').nth(1))
            .map(|s| short_path(s.trim()))
            .unwrap_or_default();
    }

    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(input) else {
        return String::new();
    };

    match name {
        "Read" | "Edit" | "Write" => string_field(&obj, &["file_path", "filePath", "path"])
            .map(short_path)
            .unwrap_or_default(),
        "Bash" => string_field(&obj, &["description", "command", "cmd"])
            .map(|s| {
                if s.len() > 60 {
                    format!("{}...", truncate_char_boundary(s, 57))
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        "Grep" | "Glob" => string_field(&obj, &["pattern", "query"])
            .unwrap_or_default()
            .to_string(),
        "Plan" => string_field(&obj, &["explanation"])
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

pub(crate) fn render_tool_input_detail(tool_name: &str, tool_input: &str) -> String {
    if (tool_name == "Apply_patch" || tool_name == "Edit") && tool_input.contains("*** Begin Patch")
    {
        let file_line = tool_input
            .lines()
            .find(|l| {
                l.starts_with("*** Add File:")
                    || l.starts_with("*** Update File:")
                    || l.starts_with("*** Delete File:")
            })
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim());
        let mut html = String::new();
        if let Some(fp) = file_line {
            html.push_str(&render_field("file", fp));
        }
        html.push_str(&render_patch_diff(tool_input));
        return html;
    }

    let parsed: Result<Value, _> = serde_json::from_str(tool_input);
    let obj = match parsed {
        Ok(Value::Object(m)) => m,
        _ => {
            return format!(r#"<pre class="tool-raw">{}</pre>"#, html_escape(tool_input));
        }
    };

    let mut html = String::new();
    match tool_name {
        "Edit" => {
            if let Some(fp) = string_field(&obj, &["file_path", "filePath"]) {
                html.push_str(&render_field("file", fp));
            }
            let old = string_field(&obj, &["old_string", "oldString"]);
            let new = string_field(&obj, &["new_string", "newString"]);
            if let (Some(old), Some(new)) = (old, new) {
                html.push_str(&render_line_diff(old, new));
            }
        }
        "Bash" => {
            if let Some(cmd) = string_field(&obj, &["command", "cmd"]) {
                html.push_str(&render_pre_field("$", cmd));
            }
        }
        "Plan" => {
            if let Some(explanation) = string_field(&obj, &["explanation"]) {
                html.push_str(&render_field("explanation", explanation));
            }
            if let Some(plan) = obj.get("plan").and_then(|v| v.as_array()) {
                html.push_str(r#"<div class="tool-plan">"#);
                for step in plan {
                    let text = step.get("step").and_then(|s| s.as_str()).unwrap_or("");
                    let status = step.get("status").and_then(|s| s.as_str()).unwrap_or("");
                    let icon = match status {
                        "completed" => "✓",
                        "in_progress" => "▸",
                        _ => "○",
                    };
                    let cls = match status {
                        "completed" => "plan-done",
                        "in_progress" => "plan-active",
                        _ => "plan-pending",
                    };
                    html.push_str(&format!(
                        r#"<div class="plan-step {cls}"><span class="plan-icon">{icon}</span> {}</div>"#,
                        html_escape(text)
                    ));
                }
                html.push_str("</div>");
            }
        }
        "Read" | "Write" => {
            if let Some(fp) = string_field(&obj, &["file_path", "filePath", "path"]) {
                html.push_str(&render_field("file", fp));
            }
        }
        "Grep" | "Glob" => {
            if let Some(p) = string_field(&obj, &["pattern", "query"]) {
                html.push_str(&render_field("pattern", p));
            }
            if let Some(path) = string_field(&obj, &["path"]) {
                html.push_str(&render_field("path", path));
            }
        }
        _ => {
            for (k, v) in obj
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k, s)))
                .take(3)
            {
                html.push_str(&render_field(k, v));
            }
        }
    }

    html
}

fn structured_record(metadata: &ToolMetadata) -> Option<&serde_json::Map<String, Value>> {
    metadata.structured.as_ref()?.as_object()
}

pub(crate) fn render_tool_result_detail(metadata: Option<&ToolMetadata>) -> String {
    let Some(metadata) = metadata else {
        return String::new();
    };
    let Some(structured) = structured_record(metadata) else {
        return String::new();
    };

    let mut html = String::new();
    if let Some(status) = metadata.status.as_deref() {
        html.push_str(&render_field("status", status));
    }

    match metadata.canonical_name.as_str() {
        "Bash" => {
            if let Some(stdout) = string_field(structured, &["stdout"]) {
                if !stdout.is_empty() {
                    html.push_str(&render_pre_field("stdout", stdout));
                }
            }
            if let Some(stderr) = string_field(structured, &["stderr"]) {
                if !stderr.is_empty() {
                    html.push_str(&render_pre_field("stderr", stderr));
                }
            }
        }
        "Edit" | "Write" => {
            if let Some(file) = string_field(structured, &["filePath", "file_path"]) {
                html.push_str(&render_field("file", file));
            }
            let old = string_field(structured, &["oldString", "old_string"]);
            let new = string_field(structured, &["newString", "new_string"]);
            if let (Some(old), Some(new)) = (old, new) {
                html.push_str(&render_line_diff(old, new));
            }
        }
        "Agent" => {
            for (label, key) in [
                ("agent", "agentId"),
                ("type", "agentType"),
                ("tokens", "totalTokens"),
                ("tools", "totalToolUseCount"),
            ] {
                if let Some(value) = structured.get(key) {
                    html.push_str(&render_field(label, &value_to_short_string(value)));
                }
            }
        }
        "ToolSearch" => {
            if let Some(query) = string_field(structured, &["query"]) {
                html.push_str(&render_field("query", query));
            }
            if let Some(matches) = structured.get("matches").and_then(|v| v.as_array()) {
                html.push_str(&render_field("matches", &matches.len().to_string()));
            }
        }
        "WebFetch" => {
            for key in ["url", "code", "codeText", "durationMs"] {
                if let Some(value) = structured.get(key) {
                    html.push_str(&render_field(key, &value_to_short_string(value)));
                }
            }
        }
        _ if metadata.category == "task" => {
            for key in ["taskId", "task_id", "statusChange", "message"] {
                if let Some(value) = structured.get(key) {
                    html.push_str(&render_field(key, &value_to_short_string(value)));
                }
            }
        }
        _ if metadata.category == "mcp" => {
            if let Some(mcp) = &metadata.mcp {
                html.push_str(&render_field("server", &mcp.server));
                html.push_str(&render_field("tool", &mcp.tool));
            }
        }
        _ => {}
    }

    html
}

fn value_to_short_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn suppress_raw_output(metadata: Option<&ToolMetadata>) -> bool {
    matches!(
        metadata.and_then(|m| m.result_kind.as_deref()),
        Some("terminal_output" | "file_patch")
    )
}

pub(crate) fn should_skip_tool(name: &str, metadata: Option<&ToolMetadata>) -> bool {
    name.starts_with("toolu_") && metadata.is_none()
}
