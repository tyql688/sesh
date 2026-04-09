use std::fs;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;

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

fn is_allowed_image_path(path: &str) -> bool {
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return false;
    };
    let home_ok = dirs::home_dir().is_some_and(|h| canonical.starts_with(&h));
    let tmp_ok = {
        #[cfg(not(target_os = "windows"))]
        {
            canonical.starts_with("/tmp")
                || canonical.starts_with("/private/tmp")
                || canonical.starts_with("/var/folders")
        }
        #[cfg(target_os = "windows")]
        {
            std::env::var("TEMP")
                .map(|t| canonical.starts_with(&t))
                .unwrap_or(false)
                || std::env::var("TMP")
                    .map(|t| canonical.starts_with(&t))
                    .unwrap_or(false)
        }
    };
    home_ok || tmp_ok
}

fn inline_image(path: &str) -> String {
    if !is_allowed_image_path(path) {
        return format!(
            r#"<div class="msg-image"><em>[Image path not allowed: {}]</em></div>"#,
            html_escape(path)
        );
    }
    let Ok(data) = std::fs::read(path) else {
        return format!(
            r#"<div class="msg-image"><em>[Image not found: {}]</em></div>"#,
            html_escape(path)
        );
    };
    let ext = path.rsplit('.').next().unwrap_or("png").to_lowercase();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    let b64 = BASE64.encode(&data);
    format!(
        r#"<div class="msg-image"><img src="data:{mime};base64,{b64}" alt="User image" style="max-width:100%;max-height:500px;border-radius:8px;border:1px solid #e5e7eb;cursor:zoom-in" onclick="openLightbox(this.src)"></div>"#
    )
}

/// Convert markdown-style code fences to styled `<pre><code>` blocks,
/// render image references as `<img>` tags, and escape HTML outside of code blocks.
/// Render markdown content to HTML using pulldown-cmark.
/// Preserves custom `[Image: source: ...]` markers via placeholder round-trip.
fn render_content(raw: &str) -> String {
    // Phase 1: Extract [Image: source: ...] markers and replace with placeholders.
    // pulldown-cmark would mangle them as broken link references.
    let mut images: Vec<String> = Vec::new();
    let mut preprocessed = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("[Image") {
        // Find closing bracket first to bound the search
        let Some(bracket_end) = rest[start..].find(']') else {
            preprocessed.push_str(&rest[..start + 6]);
            rest = &rest[start + 6..];
            continue;
        };
        let marker = &rest[start..start + bracket_end + 1];
        if let Some(src_off) = marker.find("source: ") {
            let path_begin = start + src_off + 8;
            if let Some(end) = rest[path_begin..].find(']') {
                let abs_end = path_begin + end;
                preprocessed.push_str(&rest[..start]);
                let path = rest[path_begin..abs_end].trim();
                let placeholder = format!("\n\n<!--IMG_PLACEHOLDER_{}-->\n\n", images.len());
                let img_html = if path.starts_with("data:") {
                    format!(
                        r#"<div class="msg-image"><img src="{}" alt="User image" style="max-width:100%;max-height:500px;border-radius:8px;border:1px solid #e5e7eb;cursor:zoom-in" onclick="openLightbox(this.src)"></div>"#,
                        html_escape(path)
                    )
                } else {
                    inline_image(path)
                };
                images.push(img_html);
                preprocessed.push_str(&placeholder);
                rest = &rest[abs_end + 1..];
                continue;
            }
        }
        // Malformed marker — keep as text
        preprocessed.push_str(&rest[..start + 6]);
        rest = &rest[start + 6..];
    }
    preprocessed.push_str(rest);

    // Phase 2: Render markdown to HTML via pulldown-cmark
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    opts.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(&preprocessed, opts);
    // Escape raw HTML in content (e.g. JSX tags like <Show>, <Explorer>)
    // to prevent them from breaking the bubble DOM structure.
    // Exception: IMG_PLACEHOLDER comments must pass as Html so Phase 4
    // can find and replace them with actual <img> tags.
    let safe_parser = parser.map(|event| match event {
        pulldown_cmark::Event::Html(ref html) if !html.contains("IMG_PLACEHOLDER_") => {
            pulldown_cmark::Event::Text(html.clone())
        }
        pulldown_cmark::Event::InlineHtml(html) => pulldown_cmark::Event::Text(html),
        other => other,
    });
    let mut md_html = String::new();
    pulldown_cmark::html::push_html(&mut md_html, safe_parser);

    // Phase 3: Add our CSS class to <pre> blocks for code styling
    let md_html = md_html.replace("<pre>", r#"<pre class="code-block">"#);

    // Phase 4: Replace image placeholders with actual HTML
    let mut out = md_html;
    for (i, img_html) in images.iter().enumerate() {
        let placeholder = format!("<!--IMG_PLACEHOLDER_{}-->", i);
        out = out.replace(&placeholder, img_html);
    }

    out
}

/// Render tool_input JSON as a structured HTML summary.
pub(crate) fn render_tool_detail(tool_name: &str, tool_input: &str) -> String {
    // Apply_patch: raw patch text, not JSON
    if tool_name == "Apply_patch" && tool_input.contains("*** Begin Patch") {
        // Extract file path from patch header
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
            return format!(r#"<pre class="tool-raw">{}</pre>"#, html_escape(tool_input));
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
            let cmd = obj
                .get("command")
                .or_else(|| obj.get("cmd"))
                .and_then(|v| v.as_str());
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
        "Bash" => "💻",
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
            .find(|l| {
                l.starts_with("*** Add File:")
                    || l.starts_with("*** Update File:")
                    || l.starts_with("*** Delete File:")
            })
            .and_then(|l| l.split(':').nth(1))
            .map(|s| {
                let p = s.trim();
                let path = Path::new(p);
                let components: Vec<&str> = path
                    .iter()
                    .rev()
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|c| c.to_str().unwrap_or(""))
                    .collect();
                components.join("/")
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
                let path = Path::new(p);
                let components: Vec<&str> = path
                    .iter()
                    .rev()
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|c| c.to_str().unwrap_or(""))
                    .collect();
                components.join("/")
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
            .map(|s| {
                if s.len() > 60 {
                    format!("{}...", truncate_char_boundary(s, 57))
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn user_avatar_svg() -> &'static str {
    r#"<svg width="24" height="24" fill="currentColor" viewBox="0 0 24 24"><path d="M12 12c2.7 0 4.8-2.1 4.8-4.8S14.7 2.4 12 2.4 7.2 4.5 7.2 7.2 9.3 12 12 12zm0 2.4c-3.2 0-9.6 1.6-9.6 4.8v2.4h19.2v-2.4c0-3.2-6.4-4.8-9.6-4.8z"/></svg>"#
}

fn provider_avatar_svg(provider: &Provider) -> &'static str {
    provider.descriptor().avatar_svg()
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
        |d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        },
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
        let secs = if n > 2e10 {
            (n / 1000.0) as i64
        } else {
            n as i64
        };
        if let Some(dt) = chrono::DateTime::from_timestamp(secs, 0) {
            return dt.with_timezone(&chrono::Local).format("%H:%M").to_string();
        }
    }
    raw.to_string()
}

fn fmt_file_size(bytes: u64) -> String {
    if bytes == 0 {
        return "—".to_string();
    }
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
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
    let provider_clr = detail.meta.provider.descriptor().color();
    let project = html_escape(&detail.meta.project_name);
    let count = detail.meta.message_count;
    let date = format_timestamp(detail.meta.created_at);
    let file_size = fmt_file_size(detail.meta.file_size_bytes);
    let model = detail.meta.model.as_deref().unwrap_or("");
    let cc_version = detail.meta.cc_version.as_deref().unwrap_or("");
    let git_branch = detail.meta.git_branch.as_deref().unwrap_or("");
    // Don't redact here — export_html applies redact_home_path on the full output
    let project_path = detail.meta.project_path.as_str();

    // Aggregate token totals
    let (total_input, total_output, total_cache_read, total_cache_write) = detail
        .messages
        .iter()
        .fold((0u64, 0u64, 0u64, 0u64), |(inp, out, cr, cw), msg| {
            if let Some(u) = &msg.token_usage {
                (
                    inp + u.input_tokens as u64,
                    out + u.output_tokens as u64,
                    cr + u.cache_read_input_tokens as u64,
                    cw + u.cache_creation_input_tokens as u64,
                )
            } else {
                (inp, out, cr, cw)
            }
        });
    let has_tokens = total_input > 0 || total_output > 0;

    let user_svg = user_avatar_svg();
    let assistant_svg = provider_avatar_svg(&detail.meta.provider);

    let mut messages_html = String::new();
    for (i, msg) in detail.messages.iter().enumerate() {
        let ts = msg
            .timestamp
            .as_deref()
            .map(|t| html_escape(&format_msg_ts(t)))
            .unwrap_or_default();
        let label = role_label(&msg.role);

        match msg.role {
            MessageRole::User => {
                let content_html = render_content(&msg.content);
                let msg_id = format!("msg-{i}");
                messages_html.push_str(&format!(
                    r#"<div class="msg msg-user">
  <div class="bubble bubble-user" id="{msg_id}">
    <div class="msg-header"><span class="role-label">{label}</span><span class="msg-actions"><button class="copy-btn" onclick="copyMsg('{msg_id}')" title="Copy">📋</button></span><span class="ts">{ts}</span></div>
    <div class="msg-body">{content_html}</div>
  </div>
  <div class="avatar avatar-user">{user_svg}</div>
</div>"#
                ));
            }
            MessageRole::Assistant => {
                let content_html = render_content(&msg.content);
                let msg_model = msg.model.as_deref().unwrap_or("");
                let mut meta_html_parts: Vec<String> = Vec::new();
                if !msg_model.is_empty() {
                    meta_html_parts.push(html_escape(msg_model));
                }
                if let Some(u) = &msg.token_usage {
                    let mut s = format!(
                        "↑{} ↓{}",
                        fmt_k(u.input_tokens as u64),
                        fmt_k(u.output_tokens as u64)
                    );
                    if u.cache_creation_input_tokens > 0 || u.cache_read_input_tokens > 0 {
                        s.push_str(&format!(
                            r#" · <span class="cache-read">cache_read {}</span> · cache_write {}"#,
                            fmt_k(u.cache_read_input_tokens as u64),
                            fmt_k(u.cache_creation_input_tokens as u64)
                        ));
                    }
                    meta_html_parts.push(s);
                }
                let token_row = if meta_html_parts.is_empty() {
                    String::new()
                } else {
                    format!(
                        r#"<div class="msg-token-row">{}</div>"#,
                        meta_html_parts.join(" · ")
                    )
                };
                let msg_id = format!("msg-{i}");
                messages_html.push_str(&format!(
                    r#"<div class="msg msg-assistant">
  <div class="avatar avatar-assistant">{assistant_svg}</div>
  <div class="bubble bubble-assistant" id="{msg_id}">
    <div class="msg-header"><span class="role-label">{label}</span><span class="msg-actions"><button class="copy-btn" onclick="copyMsg('{msg_id}')" title="Copy">📋</button></span><span class="ts">{ts}</span></div>
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
        let mut s = format!("↑{} ↓{} tokens", fmt_k(total_input), fmt_k(total_output));
        if total_cache_write > 0 || total_cache_read > 0 {
            s.push_str(&format!(
                r#" · <span class="cache-read">cache_read {}</span> · cache_write {}"#,
                fmt_k(total_cache_read),
                fmt_k(total_cache_write)
            ));
        }
        format!("<span>{s}</span>")
    } else {
        String::new()
    };

    let model_html = if model.is_empty() {
        String::new()
    } else {
        format!("<span>🤖 {}</span>", html_escape(model))
    };
    let version_html = if cc_version.is_empty() {
        String::new()
    } else {
        format!("<span>🏷️ {}</span>", html_escape(cc_version))
    };
    let branch_html = if git_branch.is_empty() {
        String::new()
    } else {
        format!("<span>⎇ {}</span>", html_escape(git_branch))
    };
    let path_html = if !project_path.is_empty() {
        format!("<span>📂 {}</span>", html_escape(project_path))
    } else if !project.is_empty() {
        format!("<span>📁 {}</span>", project)
    } else {
        String::new()
    };

    super::templates::assemble_html(
        &title,
        &provider_label,
        provider_clr,
        count,
        &html_escape(&date),
        &file_size,
        &messages_html,
        &token_summary_html,
        &model_html,
        &version_html,
        &branch_html,
        &path_html,
    )
}

pub fn export_html(detail: &SessionDetail, output_path: &Path) -> Result<(), String> {
    let html = super::redact_home_path(&render(detail));
    fs::write(output_path, html).map_err(|e| format!("failed to write file: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_content_escapes_code_fence_language() {
        let input = "```\"><script>alert(1)</script>\nmalicious\n```";
        let html = render_content(input);
        assert!(
            !html.contains("<script>"),
            "lang must be escaped; got: {html}"
        );
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_render_content_normal_lang() {
        let input = "```rust\nlet x = 1;\n```";
        let html = render_content(input);
        assert!(html.contains(r#"class="language-rust""#));
        assert!(html.contains("let x = 1;"));
    }

    #[test]
    fn test_render_content_renders_footnotes() {
        let input = "This has a footnote[^note].\n\n[^note]: Footnote text";
        let html = render_content(input);
        assert!(html.contains(r#"class="footnote-reference""#));
        assert!(html.contains(r#"class="footnote-definition""#));
        assert!(html.contains("Footnote text"));
    }
}
