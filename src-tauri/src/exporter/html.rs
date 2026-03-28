use std::fs;
use std::path::Path;

use crate::models::{MessageRole, Provider, SessionDetail};

/// Truncate a string at a char boundary, avoiding panic on multi-byte UTF-8.
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

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Convert markdown-style code fences to styled `<pre><code>` blocks,
/// render image references as `<img>` tags, and escape HTML outside of code blocks.
fn render_content(raw: &str) -> String {
    let mut out = String::new();
    let mut in_code_block = false;
    let mut code_buf = String::new();
    let mut lang = String::new();

    for line in raw.split('\n') {
        if !in_code_block {
            if let Some(rest) = line.strip_prefix("```") {
                in_code_block = true;
                lang = rest.trim().to_string();
                code_buf.clear();
            } else {
                // Check for image references: [Image: source: /path/to/file.png]
                // or [Image #N]
                let trimmed = line.trim();
                if trimmed.starts_with("[Image") && trimmed.ends_with(']') {
                    if let Some(path_start) = trimmed.find("source: ") {
                        let path = &trimmed[path_start + 8..trimmed.len() - 1].trim();
                        out.push_str(&format!(
                            r#"<div class="msg-image"><img src="file://{}" alt="User image" style="max-width:100%;max-height:500px;border-radius:8px;border:1px solid #e5e7eb"></div>"#,
                            html_escape(path)
                        ));
                        continue;
                    }
                }

                if !out.is_empty() {
                    out.push_str("<br>");
                }
                out.push_str(&html_escape(line));
            }
        } else if line.trim_start().starts_with("```") {
            in_code_block = false;
            let lang_attr = if lang.is_empty() {
                String::new()
            } else {
                format!(r#" class="language-{lang}""#)
            };
            out.push_str(&format!(
                r#"<pre class="code-block"><code{lang_attr}>{}</code></pre>"#,
                html_escape(code_buf.trim_end())
            ));
        } else {
            if !code_buf.is_empty() {
                code_buf.push('\n');
            }
            code_buf.push_str(line);
        }
    }

    if in_code_block && !code_buf.is_empty() {
        out.push_str(&format!(
            r#"<pre class="code-block"><code>{}</code></pre>"#,
            html_escape(code_buf.trim_end())
        ));
    }

    out
}

/// Render tool_input JSON as a structured HTML summary.
fn render_tool_detail(tool_name: &str, tool_input: &str) -> String {
    // Apply_patch: raw patch text, not JSON
    if tool_name == "Apply_patch" && tool_input.contains("*** Begin Patch") {
        // Extract file path from patch header
        let file_line = tool_input
            .lines()
            .find(|l| l.starts_with("*** Add File:") || l.starts_with("*** Update File:") || l.starts_with("*** Delete File:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim());
        let mut html = String::new();
        if let Some(fp) = file_line {
            html.push_str(&format!(
                r#"<div class="tool-field"><span class="tool-field-label">file</span><span class="tool-field-value">{}</span></div>"#,
                html_escape(fp)
            ));
        }
        html.push_str(&format!(
            r#"<pre class="tool-raw">{}</pre>"#,
            html_escape(tool_input)
        ));
        return html;
    }

    let parsed: Result<serde_json::Value, _> = serde_json::from_str(tool_input);
    let obj = match parsed {
        Ok(serde_json::Value::Object(m)) => m,
        _ => {
            return format!(
                r#"<pre class="tool-raw">{}</pre>"#,
                html_escape(tool_input)
            );
        }
    };

    let mut html = String::new();

    match tool_name {
        "Edit" => {
            if let Some(fp) = obj.get("file_path").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-field"><span class="tool-field-label">file</span><span class="tool-field-value">{}</span></div>"#,
                    html_escape(fp)
                ));
            }
            if let Some(old) = obj.get("old_string").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-diff tool-diff-old"><span class="tool-diff-label">−</span><pre>{}</pre></div>"#,
                    html_escape(old)
                ));
            }
            if let Some(new) = obj.get("new_string").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-diff tool-diff-new"><span class="tool-diff-label">+</span><pre>{}</pre></div>"#,
                    html_escape(new)
                ));
            }
        }
        "Bash" => {
            let cmd = obj.get("command").or_else(|| obj.get("cmd")).and_then(|v| v.as_str());
            if let Some(cmd) = cmd {
                html.push_str(&format!(
                    r#"<div class="tool-field"><span class="tool-field-label">$</span><pre class="tool-cmd">{}</pre></div>"#,
                    html_escape(cmd)
                ));
            }
        }
        "Plan" => {
            if let Some(explanation) = obj.get("explanation").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-field"><span class="tool-field-label">explanation</span><span class="tool-field-value">{}</span></div>"#,
                    html_escape(explanation)
                ));
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
            if let Some(fp) = obj.get("file_path").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-field"><span class="tool-field-label">file</span><span class="tool-field-value">{}</span></div>"#,
                    html_escape(fp)
                ));
            }
        }
        "Grep" | "Glob" => {
            if let Some(p) = obj.get("pattern").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-field"><span class="tool-field-label">pattern</span><span class="tool-field-value">{}</span></div>"#,
                    html_escape(p)
                ));
            }
            if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                html.push_str(&format!(
                    r#"<div class="tool-field"><span class="tool-field-label">path</span><span class="tool-field-value">{}</span></div>"#,
                    html_escape(path)
                ));
            }
        }
        _ => {
            // Generic: show first few string fields
            let mut count = 0;
            for (k, v) in &obj {
                if count >= 3 {
                    break;
                }
                if let Some(s) = v.as_str() {
                    html.push_str(&format!(
                        r#"<div class="tool-field"><span class="tool-field-label">{}</span><span class="tool-field-value">{}</span></div>"#,
                        html_escape(k), html_escape(s)
                    ));
                    count += 1;
                }
            }
        }
    }

    html
}

fn tool_icon(name: &str) -> &'static str {
    match name {
        "Read" => "📄",
        "Edit" | "Apply_patch" => "✏️",
        "Write" => "📝",
        "Bash" => "⬛",
        "Glob" => "🔍",
        "Grep" => "🔎",
        "Agent" => "🤖",
        "Plan" => "📋",
        _ => "⚙",
    }
}

/// Short display name from file_path or command.
fn tool_summary(name: &str, input: &str) -> String {
    // Apply_patch: raw patch text, extract file path
    if name == "Apply_patch" {
        return input
            .lines()
            .find(|l| l.starts_with("*** Add File:") || l.starts_with("*** Update File:") || l.starts_with("*** Delete File:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| {
                let p = s.trim();
                let parts: Vec<&str> = p.split('/').collect();
                parts[parts.len().saturating_sub(2)..].join("/")
            })
            .unwrap_or_default();
    }

    let parsed: Result<serde_json::Value, _> = serde_json::from_str(input);
    let obj = match parsed {
        Ok(serde_json::Value::Object(m)) => m,
        _ => return String::new(),
    };
    match name {
        "Read" | "Edit" | "Write" => obj
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|p| {
                let parts: Vec<&str> = p.split('/').collect();
                parts[parts.len().saturating_sub(2)..].join("/")
            })
            .unwrap_or_default(),
        "Bash" => obj
            .get("description")
            .or_else(|| obj.get("command"))
            .or_else(|| obj.get("cmd"))
            .and_then(|v| v.as_str())
            .map(|s| {
                if s.len() > 60 {
                    format!("{}...", truncate_char_boundary(s, 57))
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        "Grep" | "Glob" => obj
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "Plan" => obj
            .get("explanation")
            .and_then(|v| v.as_str())
            .map(|s| if s.len() > 60 { format!("{}...", truncate_char_boundary(s, 57)) } else { s.to_string() })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn provider_color(provider: &Provider) -> &'static str {
    provider.color()
}

fn user_avatar_svg() -> &'static str {
    r#"<svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path d="M12 12c2.7 0 4.8-2.1 4.8-4.8S14.7 2.4 12 2.4 7.2 4.5 7.2 7.2 9.3 12 12 12zm0 2.4c-3.2 0-9.6 1.6-9.6 4.8v2.4h19.2v-2.4c0-3.2-6.4-4.8-9.6-4.8z"/></svg>"#
}

fn provider_avatar_svg(provider: &Provider) -> &'static str {
    match provider {
        Provider::Claude => r#"<svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path d="M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z"/></svg>"#,
        Provider::Codex => r#"<svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z"/></svg>"#,
        Provider::Gemini => r#"<svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path d="M11.04 19.32Q12 21.51 12 24q0-2.49.93-4.68.96-2.19 2.58-3.81t3.81-2.55Q21.51 12 24 12q-2.49 0-4.68-.93a12.3 12.3 0 0 1-3.81-2.58 12.3 12.3 0 0 1-2.58-3.81Q12 2.49 12 0q0 2.49-.96 4.68-.93 2.19-2.55 3.81a12.3 12.3 0 0 1-3.81 2.58Q2.49 12 0 12q2.49 0 4.68.96 2.19.93 3.81 2.55t2.55 3.81"/></svg>"#,
        Provider::Cursor => r#"<svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path d="M11.503.131 1.891 5.678a.84.84 0 0 0-.42.726v11.188c0 .3.162.575.42.724l9.609 5.55a1 1 0 0 0 .998 0l9.61-5.55a.84.84 0 0 0 .42-.724V6.404a.84.84 0 0 0-.42-.726L12.497.131a1.01 1.01 0 0 0-.996 0M2.657 6.338h18.55c.263 0 .43.287.297.515L12.23 22.918c-.062.107-.229.064-.229-.06V12.335a.59.59 0 0 0-.295-.51l-9.11-5.257c-.109-.063-.064-.23.061-.23"/></svg>"#,
        Provider::OpenCode => r#"<svg width="16" height="16" fill="currentColor" viewBox="0 0 24 24"><path fill-rule="evenodd" clip-rule="evenodd" d="M18 19.5H6V4.5H18V19.5ZM15 7.5H9V16.5H15V7.5Z"/></svg>"#,
    }
}

fn role_label(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::User => "You",
        MessageRole::Assistant => "Assistant",
        MessageRole::Tool => "Tool",
        MessageRole::System => "System",
    }
}

fn format_timestamp(epoch: i64) -> String {
    chrono::DateTime::from_timestamp(epoch, 0).map_or_else(
        || "—".to_string(),
        |d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M").to_string(),
    )
}

/// Format a message-level timestamp string (RFC3339 or epoch) to local HH:MM.
fn format_msg_ts(raw: &str) -> String {
    // Try RFC3339 first
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return dt.with_timezone(&chrono::Local).format("%H:%M").to_string();
    }
    // Try epoch seconds/ms
    if let Ok(n) = raw.parse::<f64>() {
        let secs = if n > 2e10 { (n / 1000.0) as i64 } else { n as i64 };
        if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0) {
            return dt.with_timezone(&chrono::Local).format("%H:%M").to_string();
        }
    }
    raw.to_string()
}

fn fmt_k(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn render(detail: &SessionDetail) -> String {
    let title = html_escape(&detail.meta.title);
    let provider_label = html_escape(detail.meta.provider.label());
    let provider_clr = provider_color(&detail.meta.provider);
    let project = html_escape(&detail.meta.project_name);
    let count = detail.meta.message_count;
    let date = format_timestamp(detail.meta.created_at);

    // Aggregate token totals
    let (total_input, total_output, total_cache_read) = detail.messages.iter().fold(
        (0u64, 0u64, 0u64),
        |(inp, out, cr), msg| {
            if let Some(u) = &msg.token_usage {
                (
                    inp + u.input_tokens as u64,
                    out + u.output_tokens as u64,
                    cr + u.cache_read_input_tokens as u64,
                )
            } else {
                (inp, out, cr)
            }
        },
    );
    let has_tokens = total_input > 0 || total_output > 0;

    let user_svg = user_avatar_svg();
    let assistant_svg = provider_avatar_svg(&detail.meta.provider);

    let mut messages_html = String::new();
    for msg in &detail.messages {
        let ts = msg.timestamp.as_deref().map(|t| html_escape(&format_msg_ts(t))).unwrap_or_default();
        let label = role_label(&msg.role);

        match msg.role {
            MessageRole::User => {
                let content_html = render_content(&msg.content);
                messages_html.push_str(&format!(
                    r#"<div class="msg msg-user">
  <div class="bubble bubble-user">
    <div class="msg-header"><span class="role-label">{label}</span><span class="ts">{ts}</span></div>
    <div class="msg-body">{content_html}</div>
  </div>
  <div class="avatar avatar-user">{user_svg}</div>
</div>"#
                ));
            }
            MessageRole::Assistant => {
                let content_html = render_content(&msg.content);
                let token_row = if let Some(u) = &msg.token_usage {
                    let mut parts = vec![
                        format!("↑{}", fmt_k(u.input_tokens as u64)),
                        format!("↓{}", fmt_k(u.output_tokens as u64)),
                    ];
                    if u.cache_read_input_tokens > 0 {
                        parts.push(format!("cache_read {}", fmt_k(u.cache_read_input_tokens as u64)));
                    }
                    if u.cache_creation_input_tokens > 0 {
                        parts.push(format!("cache_write {}", fmt_k(u.cache_creation_input_tokens as u64)));
                    }
                    format!(r#"<div class="msg-token-row">{}</div>"#, html_escape(&parts.join(" · ")))
                } else {
                    String::new()
                };
                messages_html.push_str(&format!(
                    r#"<div class="msg msg-assistant">
  <div class="avatar avatar-assistant">{assistant_svg}</div>
  <div class="bubble bubble-assistant">
    <div class="msg-header"><span class="role-label">{label}</span><span class="ts">{ts}</span></div>
    <div class="msg-body">{content_html}</div>
  </div>
</div>{token_row}"#
                ));
            }
            MessageRole::Tool => {
                let name = msg.tool_name.as_deref().unwrap_or("tool");
                // Skip tool_result entries (toolu_ IDs from Anthropic API)
                if name.starts_with("toolu_") {
                    continue;
                }
                let icon = tool_icon(name);
                let has_input = msg
                    .tool_input
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty());
                let has_output = !msg.content.trim().is_empty();
                let summary = if has_input {
                    tool_summary(name, msg.tool_input.as_deref().unwrap_or(""))
                } else {
                    String::new()
                };
                let summary_html = if summary.is_empty() {
                    String::new()
                } else {
                    format!(
                        r#"<span class="tool-hint">{}</span>"#,
                        html_escape(&summary)
                    )
                };

                let mut detail_html = String::new();
                if has_input {
                    detail_html.push_str(&render_tool_detail(
                        name,
                        msg.tool_input.as_deref().unwrap_or(""),
                    ));
                }
                if has_output {
                    let content_html = render_content(&msg.content);
                    detail_html
                        .push_str(&format!(r#"<div class="tool-output">{content_html}</div>"#));
                }

                if detail_html.is_empty() {
                    messages_html.push_str(&format!(
                        r#"<div class="msg msg-tool">
  <div class="tool-block-closed"><span class="tool-icon">{icon}</span><span class="tool-name">{name}</span>{summary_html}</div>
</div>"#
                    ));
                } else {
                    messages_html.push_str(&format!(
                        r#"<div class="msg msg-tool">
  <details class="tool-block">
    <summary class="tool-summary"><span class="tool-icon">{icon}</span><span class="tool-name">{name}</span>{summary_html}</summary>
    <div class="tool-content">{detail_html}</div>
  </details>
</div>"#
                    ));
                }
            }
            MessageRole::System => {
                if msg.content.starts_with("[thinking]\n") {
                    let thinking_text = &msg.content["[thinking]\n".len()..];
                    let content_html = render_content(thinking_text);
                    messages_html.push_str(&format!(
                        r#"<div class="msg msg-thinking">
  <details class="thinking-block">
    <summary class="thinking-summary">💭 Thinking</summary>
    <div class="thinking-content">{content_html}</div>
  </details>
</div>"#
                    ));
                } else {
                    let content_html = render_content(&msg.content);
                    messages_html.push_str(&format!(
                        r#"<div class="msg msg-system">
  <div class="system-text">{content_html}</div>
</div>"#
                    ));
                }
            }
        }
    }

    let token_summary_html = if has_tokens {
        let mut s = format!("🔢 ↑{} ↓{}", fmt_k(total_input), fmt_k(total_output));
        if total_cache_read > 0 {
            s.push_str(&format!(" cache_read {}", fmt_k(total_cache_read)));
        }
        format!("<span>{}</span>", html_escape(&s))
    } else {
        String::new()
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta name="generator" content="CC Session — AI Session Explorer">
<meta name="color-scheme" content="light dark">
<link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>💬</text></svg>">
<title>{title}</title>
<style>
*,*::before,*::after {{ box-sizing: border-box; }}
:root {{ --bg: #f9fafb; --bg-bubble: #fff; --text: #1a1a1a; --text2: #6b7280; --text3: #9ca3af; --border: #e5e7eb; --code-bg: #1e1e2e; --code-fg: #cdd6f4; --diff-old: rgba(239,68,68,0.12); --diff-new: rgba(34,197,94,0.12); }}
@media (prefers-color-scheme: dark) {{
  :root {{ --bg: #111; --bg-bubble: #1c1c1e; --text: #e5e5e5; --text2: #9ca3af; --text3: #6b7280; --border: #333; --code-bg: #0d0d0d; --code-fg: #cdd6f4; --diff-old: rgba(239,68,68,0.15); --diff-new: rgba(34,197,94,0.15); }}
}}
body {{ font-family: -apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,'Helvetica Neue',Arial,sans-serif; font-size: 15px; line-height: 1.6; color: var(--text); background: var(--bg); margin: 0; padding: 0; }}
.container {{ max-width: 1280px; margin: 0 auto; padding: 32px 24px 64px; }}
.header {{ padding: 40px 0 28px; border-bottom: 1px solid var(--border); margin-bottom: 36px; }}
.header h1 {{ font-size: 1.6em; font-weight: 700; margin: 0 0 16px; line-height: 1.3; }}
.header-meta {{ display: flex; flex-wrap: wrap; gap: 12px; align-items: center; font-size: 0.85em; color: var(--text2); }}
.badge {{ display: inline-block; padding: 2px 10px; border-radius: 12px; font-size: 0.8em; font-weight: 600; color: #fff; }}
.messages {{ display: flex; flex-direction: column; gap: 16px; }}
.msg {{ display: flex; align-items: flex-start; gap: 10px; }}
.msg-user {{ flex-direction: row-reverse; }}
.msg-tool {{ padding-left: 44px; }}
.msg-system {{ justify-content: center; }}
.avatar {{ width: 34px; height: 34px; border-radius: 50%; display: flex; align-items: center; justify-content: center; font-size: 0.8em; font-weight: 700; color: #fff; flex-shrink: 0; margin-top: 4px; }}
.avatar-user {{ background: linear-gradient(135deg, #007aff, #5856d6); }}
.avatar-assistant {{ background: linear-gradient(135deg, {provider_clr}, {provider_clr}cc); }}
.bubble {{ max-width: 85%; padding: 12px 16px; border-radius: 16px; word-wrap: break-word; overflow-wrap: break-word; }}
.bubble-user {{ background: #007aff; color: #fff; border-bottom-right-radius: 4px; }}
.bubble-user .ts, .bubble-user .role-label {{ color: rgba(255,255,255,0.7); }}
.bubble-user a {{ color: #b3d9ff; }}
.bubble-assistant {{ background: var(--bg-bubble); border: 1px solid var(--border); color: var(--text); border-bottom-left-radius: 4px; }}
.msg-header {{ display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px; gap: 8px; }}
.role-label {{ font-size: 0.75em; font-weight: 600; color: var(--text2); }}
.ts {{ font-size: 0.7em; color: var(--text3); white-space: nowrap; }}
.msg-body {{ font-size: 0.95em; }}
/* Tool blocks */
.tool-block, .tool-block-closed {{ max-width: 90%; background: var(--bg-bubble); border: 1px solid var(--border); border-radius: 10px; font-size: 0.85em; }}
.tool-block-closed {{ padding: 8px 14px; display: flex; align-items: center; gap: 6px; color: var(--text2); }}
.tool-summary {{ padding: 8px 14px; cursor: pointer; color: var(--text2); display: flex; align-items: center; gap: 6px; user-select: none; list-style: none; }}
.tool-summary::-webkit-details-marker {{ display: none; }}
.tool-summary:hover {{ color: var(--text); }}
.tool-icon {{ font-size: 1em; }}
.tool-name {{ font-family: 'SF Mono',Menlo,monospace; font-weight: 600; color: var(--text); }}
.tool-hint {{ color: var(--text3); font-size: 0.9em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }}
.tool-content {{ padding: 8px 14px; border-top: 1px solid var(--border); }}
.tool-field {{ display: flex; gap: 8px; padding: 3px 0; font-size: 0.9em; }}
.tool-field-label {{ color: var(--text3); font-size: 0.85em; font-weight: 600; text-transform: uppercase; min-width: 50px; flex-shrink: 0; }}
.tool-field-value {{ font-family: 'SF Mono',Menlo,monospace; color: var(--text); word-break: break-all; }}
.tool-cmd {{ margin: 0; font-family: 'SF Mono',Menlo,monospace; white-space: pre-wrap; color: var(--text); }}
.tool-diff {{ display: flex; border-radius: 4px; overflow: hidden; margin: 4px 0; }}
.tool-diff pre {{ margin: 0; padding: 6px 8px; font-family: 'SF Mono',Menlo,monospace; font-size: 0.88em; line-height: 1.4; white-space: pre-wrap; word-break: break-word; max-height: 200px; overflow-y: auto; flex: 1; }}
.tool-diff-old {{ background: var(--diff-old); }}
.tool-diff-new {{ background: var(--diff-new); }}
.tool-diff-label {{ padding: 6px; font-family: 'SF Mono',Menlo,monospace; font-weight: 700; flex-shrink: 0; }}
.tool-diff-old .tool-diff-label {{ color: #ef4444; }}
.tool-diff-new .tool-diff-label {{ color: #22c55e; }}
.tool-output {{ border-top: 1px solid var(--border); padding: 6px 0; font-family: 'SF Mono',Menlo,monospace; font-size: 0.88em; color: var(--text2); white-space: pre-wrap; max-height: 200px; overflow-y: auto; }}
.tool-raw {{ margin: 0; font-size: 0.88em; white-space: pre-wrap; word-break: break-word; color: var(--text2); }}
.system-text {{ font-size: 0.8em; color: var(--text3); text-align: center; padding: 4px 16px; max-width: 70%; }}
.code-block {{ background: var(--code-bg); color: var(--code-fg); border-radius: 8px; padding: 14px 16px; margin: 8px 0; overflow-x: auto; font-family: 'SF Mono',Menlo,monospace; font-size: 0.88em; line-height: 1.5; }}
.code-block code {{ background: none; padding: 0; color: inherit; }}
.bubble-user .code-block {{ background: rgba(0,0,0,0.25); color: #e8eaed; }}
.msg-image {{ margin: 8px 0; }}
.msg-image img {{ border-radius: 8px; border: 1px solid var(--border); }}
.msg-token-row {{ padding-left: 44px; font-size: 0.78em; color: var(--text3); font-variant-numeric: tabular-nums; margin-top: -12px; }}
.tool-plan {{ padding: 4px 0; }}
.plan-step {{ padding: 3px 0; font-size: 0.9em; }}
.plan-icon {{ font-family: monospace; margin-right: 4px; }}
.plan-done {{ color: #22c55e; }}
.plan-active {{ color: var(--text); font-weight: 600; }}
.plan-pending {{ color: var(--text3); }}
.msg-thinking {{ padding-left: 44px; }}
.thinking-block {{ max-width: 90%; background: var(--bg-bubble); border: 1px solid var(--border); border-radius: 10px; font-size: 0.85em; }}
.thinking-summary {{ padding: 8px 14px; cursor: pointer; color: var(--text3); display: flex; align-items: center; gap: 6px; user-select: none; list-style: none; font-style: italic; }}
.thinking-summary::-webkit-details-marker {{ display: none; }}
.thinking-summary:hover {{ color: var(--text2); }}
.thinking-content {{ padding: 8px 14px; border-top: 1px solid var(--border); color: var(--text2); font-size: 0.95em; line-height: 1.6; white-space: pre-wrap; }}
@media print {{
  body {{ background: #fff; font-size: 12px; }}
  .container {{ max-width: 100%; padding: 0; }}
  .bubble-user {{ background: #007aff !important; color: #fff !important; -webkit-print-color-adjust: exact; print-color-adjust: exact; }}
  .code-block {{ background: #f3f4f6 !important; color: #1a1a1a !important; border: 1px solid #ccc; }}
  .tool-block {{ break-inside: avoid; }}
  details[open] > summary {{ display: none; }}
}}
@media (max-width: 600px) {{
  .bubble, .tool-block, .tool-block-closed, .system-text {{ max-width: 95%; }}
  .container {{ padding: 12px 8px 48px; }}
  .header h1 {{ font-size: 1.2em; }}
}}
</style>
</head>
<body>
<div class="container">
  <div class="header">
    <h1>{title}</h1>
    <div class="header-meta">
      <span class="badge" style="background:{provider_clr}">{provider_label}</span>
      <span>📁 {project}</span>
      <span>💬 {count} messages</span>
      <span>📅 {date}</span>
      {token_summary_html}
    </div>
  </div>
  <div class="messages">
{messages_html}
  </div>
</div>
</body>
</html>"#,
        title = title,
        provider_clr = provider_clr,
        provider_label = provider_label,
        project = project,
        count = count,
        date = html_escape(&date),
        messages_html = messages_html,
        token_summary_html = token_summary_html,
    )
}

pub fn export_html(detail: &SessionDetail, output_path: &Path) -> Result<(), String> {
    let html = render(detail);
    fs::write(output_path, html).map_err(|e| format!("failed to write file: {e}"))?;
    Ok(())
}
