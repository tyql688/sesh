use serde_json::Value;
use similar::{ChangeTag, TextDiff};

use crate::models::ToolMetadata;
use crate::provider_utils::shorten_home_path;

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

fn is_path_label(label: &str) -> bool {
    let label = label.to_ascii_lowercase();
    label == "file" || label == "path" || label.ends_with("path")
}

fn string_field<'a>(obj: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| obj.get(*key).and_then(|v| v.as_str()))
}

fn render_field(label: &str, value: &str) -> String {
    let display_value = if is_path_label(label) {
        shorten_home_path(value)
    } else {
        value.to_string()
    };
    format!(
        r#"<div class="tool-field"><span class="tool-field-label">{}</span><span class="tool-field-value">{}</span></div>"#,
        html_escape(label),
        html_escape(&display_value)
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
            html.push_str(&render_diff_line("skip", &shorten_home_path(line)));
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

fn render_structured_patch_diff(structured_patch: &Value) -> String {
    let Some(hunks) = structured_patch.as_array() else {
        return String::new();
    };
    if hunks.is_empty() {
        return String::new();
    }

    let mut html = String::from(r#"<div class="tool-line-diff">"#);
    let mut rendered_lines = 0usize;

    for hunk in hunks {
        let Some(lines) = hunk.get("lines").and_then(|value| value.as_array()) else {
            continue;
        };
        let old_start = hunk.get("oldStart").and_then(|value| value.as_i64());
        let old_lines = hunk.get("oldLines").and_then(|value| value.as_i64());
        let new_start = hunk.get("newStart").and_then(|value| value.as_i64());
        let new_lines = hunk.get("newLines").and_then(|value| value.as_i64());
        if let (Some(old_start), Some(old_lines), Some(new_start), Some(new_lines)) =
            (old_start, old_lines, new_start, new_lines)
        {
            html.push_str(&render_diff_line(
                "skip",
                &format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
            ));
        } else {
            html.push_str(&render_diff_line("skip", "@@"));
        }

        for raw_line in lines.iter().filter_map(|line| line.as_str()) {
            if let Some(rest) = raw_line.strip_prefix('+') {
                html.push_str(&render_diff_line("add", rest));
            } else if let Some(rest) = raw_line.strip_prefix('-') {
                html.push_str(&render_diff_line("remove", rest));
            } else if let Some(rest) = raw_line.strip_prefix(' ') {
                html.push_str(&render_diff_line("context", rest));
            } else {
                html.push_str(&render_diff_line("skip", raw_line));
            }
            rendered_lines += 1;
        }
    }

    html.push_str("</div>");
    if rendered_lines == 0 {
        String::new()
    } else {
        html
    }
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
    let trimmed_input = input.trim_start();
    if (name == "Apply_patch" || name == "Edit")
        && !trimmed_input.starts_with('{')
        && input.contains("*** Begin Patch")
    {
        return input
            .lines()
            .find(|l| {
                l.starts_with("*** Add File:")
                    || l.starts_with("*** Update File:")
                    || l.starts_with("*** Delete File:")
            })
            .and_then(|l| l.split(':').nth(1))
            .map(|s| shorten_home_path(s.trim()))
            .unwrap_or_default();
    }

    let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(input) else {
        return String::new();
    };

    match name {
        "Read" | "Edit" | "Write" => string_field(&obj, &["file_path", "filePath", "path"])
            .map(shorten_home_path)
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
    let trimmed_tool_input = tool_input.trim_start();
    if (tool_name == "Apply_patch" || tool_name == "Edit")
        && !trimmed_tool_input.starts_with('{')
        && tool_input.contains("*** Begin Patch")
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
            if let Some(patch) = string_field(&obj, &["patch"]) {
                html.push_str(&render_patch_diff(patch));
                return html;
            }
            let old = string_field(&obj, &["old_string", "oldString"]);
            let new = string_field(&obj, &["new_string", "newString"]);
            if old.is_some() || new.is_some() {
                html.push_str(&render_line_diff(old.unwrap_or(""), new.unwrap_or("")));
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
            let structured_patch_html = structured
                .get("structuredPatch")
                .map(render_structured_patch_diff)
                .unwrap_or_default();
            let has_structured_patch = !structured_patch_html.is_empty();
            html.push_str(&structured_patch_html);
            let old = string_field(structured, &["oldString", "old_string"]);
            let new = string_field(structured, &["newString", "new_string"]);
            if !has_structured_patch && (old.is_some() || new.is_some()) {
                html.push_str(&render_line_diff(old.unwrap_or(""), new.unwrap_or("")));
            } else if !has_structured_patch
                && structured.get("type").and_then(|value| value.as_str()) == Some("create")
            {
                if let Some(content) = string_field(structured, &["content"]) {
                    if !content.is_empty() {
                        html.push_str(&render_line_diff("", content));
                    }
                }
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

pub(crate) fn suppress_raw_output(metadata: Option<&ToolMetadata>, result_has_diff: bool) -> bool {
    match metadata.and_then(|m| m.result_kind.as_deref()) {
        Some("terminal_output") => true,
        Some("file_patch") => result_has_diff,
        _ => false,
    }
}

pub(crate) fn should_skip_tool(name: &str, metadata: Option<&ToolMetadata>) -> bool {
    name.starts_with("toolu_") && metadata.is_none()
}
