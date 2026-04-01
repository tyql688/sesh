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
        r#"<div class="msg-image"><img src="data:{mime};base64,{b64}" alt="User image" style="max-width:100%;max-height:500px;border-radius:8px;border:1px solid #e5e7eb"></div>"#
    )
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
                // Check for image references inline: [Image: source: /path/to/file.png]
                // They may appear anywhere in the line, possibly with surrounding text.
                if line.contains("[Image") && line.contains("source: ") {
                    if !out.is_empty() {
                        out.push_str("<br>");
                    }
                    let line_str = line;
                    let mut pos = 0;
                    while pos < line_str.len() {
                        if let Some(start) = line_str[pos..].find("[Image") {
                            let abs_start = pos + start;
                            // Find the closing ']' after "source: "
                            if let Some(src_off) = line_str[abs_start..].find("source: ") {
                                let path_begin = abs_start + src_off + 8;
                                if let Some(end) = line_str[path_begin..].find(']') {
                                    let abs_end = path_begin + end;
                                    // Emit text before the image marker
                                    if abs_start > pos {
                                        out.push_str(&html_escape(&line_str[pos..abs_start]));
                                    }
                                    let path = line_str[path_begin..abs_end].trim();
                                    if path.starts_with("data:") {
                                        out.push_str(&format!(
                                            r#"<div class="msg-image"><img src="{}" alt="User image" style="max-width:100%;max-height:500px;border-radius:8px;border:1px solid #e5e7eb"></div>"#,
                                            html_escape(path)
                                        ));
                                    } else {
                                        out.push_str(&inline_image(path));
                                    }
                                    pos = abs_end + 1; // skip past ']'
                                    continue;
                                }
                            }
                            // Malformed marker — emit as text
                            out.push_str(&html_escape(&line_str[pos..pos + start + 6]));
                            pos += start + 6;
                        } else {
                            break;
                        }
                    }
                    // Emit any trailing text after last image marker
                    if pos < line_str.len() {
                        out.push_str(&html_escape(&line_str[pos..]));
                    }
                    continue;
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

fn provider_color(provider: &Provider) -> &'static str {
    match provider {
        Provider::Claude => "#8b5cf6",
        Provider::Codex => "#10b981",
        Provider::Gemini => "#f59e0b",
        Provider::Cursor => "#3b82f6",
        Provider::OpenCode => "#06b6d4",
        Provider::Kimi => "#6366f1",
        Provider::CcMirror => "#f472b6",
    }
}

fn user_avatar_svg() -> &'static str {
    r#"<svg width="24" height="24" fill="currentColor" viewBox="0 0 24 24"><path d="M12 12c2.7 0 4.8-2.1 4.8-4.8S14.7 2.4 12 2.4 7.2 4.5 7.2 7.2 9.3 12 12 12zm0 2.4c-3.2 0-9.6 1.6-9.6 4.8v2.4h19.2v-2.4c0-3.2-6.4-4.8-9.6-4.8z"/></svg>"#
}

fn provider_avatar_svg(provider: &Provider) -> &'static str {
    match provider {
        Provider::Claude => {
            r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z" fill="#D97757" fill-rule="nonzero"/></svg>"##
        }
        Provider::Codex => {
            r#"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M21.55 10.004a5.416 5.416 0 00-.478-4.501c-1.217-2.09-3.662-3.166-6.05-2.66A5.59 5.59 0 0010.831 1C8.39.995 6.224 2.546 5.473 4.838A5.553 5.553 0 001.76 7.496a5.487 5.487 0 00.691 6.5 5.416 5.416 0 00.477 4.502c1.217 2.09 3.662 3.165 6.05 2.66A5.586 5.586 0 0013.168 23c2.443.006 4.61-1.546 5.361-3.84a5.553 5.553 0 003.715-2.66 5.488 5.488 0 00-.693-6.497v.001zm-8.381 11.558a4.199 4.199 0 01-2.675-.954c.034-.018.093-.05.132-.074l4.44-2.53a.71.71 0 00.364-.623v-6.176l1.877 1.069c.02.01.033.029.036.05v5.115c-.003 2.274-1.87 4.118-4.174 4.123zM4.192 17.78a4.059 4.059 0 01-.498-2.763c.032.02.09.055.131.078l4.44 2.53c.225.13.504.13.73 0l5.42-3.088v2.138a.068.068 0 01-.027.057L9.9 19.288c-1.999 1.136-4.552.46-5.707-1.51h-.001zM3.023 8.216A4.15 4.15 0 015.198 6.41l-.002.151v5.06a.711.711 0 00.364.624l5.42 3.087-1.876 1.07a.067.067 0 01-.063.005l-4.489-2.559c-1.995-1.14-2.679-3.658-1.53-5.63h.001zm15.417 3.54l-5.42-3.088L14.896 7.6a.067.067 0 01.063-.006l4.489 2.557c1.998 1.14 2.683 3.662 1.529 5.633a4.163 4.163 0 01-2.174 1.807V12.38a.71.71 0 00-.363-.623zm1.867-2.773a6.04 6.04 0 00-.132-.078l-4.44-2.53a.731.731 0 00-.729 0l-5.42 3.088V7.325a.068.068 0 01.027-.057L14.1 4.713c2-1.137 4.555-.46 5.707 1.513.487.833.664 1.809.499 2.757h.001zm-11.741 3.81l-1.877-1.068a.065.065 0 01-.036-.051V6.559c.001-2.277 1.873-4.122 4.181-4.12.976 0 1.92.338 2.671.954-.034.018-.092.05-.131.073l-4.44 2.53a.71.71 0 00-.365.623l-.003 6.173v.002zm1.02-2.168L12 9.25l2.414 1.375v2.75L12 14.75l-2.415-1.375v-2.75z" fill="currentColor"/></svg>"#
        }
        Provider::Gemini => {
            r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="#3186FF"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-0)"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-1)"/><path d="M20.616 10.835a14.147 14.147 0 01-4.45-3.001 14.111 14.111 0 01-3.678-6.452.503.503 0 00-.975 0 14.134 14.134 0 01-3.679 6.452 14.155 14.155 0 01-4.45 3.001c-.65.28-1.318.505-2.002.678a.502.502 0 000 .975c.684.172 1.35.397 2.002.677a14.147 14.147 0 014.45 3.001 14.112 14.112 0 013.679 6.453.502.502 0 00.975 0c.172-.685.397-1.351.677-2.003a14.145 14.145 0 013.001-4.45 14.113 14.113 0 016.453-3.678.503.503 0 000-.975 13.245 13.245 0 01-2.003-.678z" fill="url(#lobe-icons-gemini-fill-2)"/><defs><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-0" x1="7" x2="11" y1="15.5" y2="12"><stop stop-color="#08B962"/><stop offset="1" stop-color="#08B962" stop-opacity="0"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-1" x1="8" x2="11.5" y1="5.5" y2="11"><stop stop-color="#F94543"/><stop offset="1" stop-color="#F94543" stop-opacity="0"/></linearGradient><linearGradient gradientUnits="userSpaceOnUse" id="lobe-icons-gemini-fill-2" x1="3.5" x2="17.5" y1="13.5" y2="12"><stop stop-color="#FABC12"/><stop offset=".46" stop-color="#FABC12" stop-opacity="0"/></linearGradient></defs></svg>"##
        }
        Provider::Cursor => {
            r#"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M22.106 5.68L12.5.135a.998.998 0 00-.998 0L1.893 5.68a.84.84 0 00-.419.726v11.186c0 .3.16.577.42.727l9.607 5.547a.999.999 0 00.998 0l9.608-5.547a.84.84 0 00.42-.727V6.407a.84.84 0 00-.42-.726zm-.603 1.176L12.228 22.92c-.063.108-.228.064-.228-.061V12.34a.59.59 0 00-.295-.51l-9.11-5.26c-.107-.062-.063-.228.062-.228h18.55c.264 0 .428.286.296.514z" fill="currentColor"/></svg>"#
        }
        Provider::OpenCode => {
            r#"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M16 6H8v12h8V6zm4 16H4V2h16v20z" fill="currentColor"/></svg>"#
        }
        Provider::Kimi => {
            r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M19.738 5.776c.163-.209.306-.4.457-.585.07-.087.064-.153-.004-.244-.655-.861-.717-1.817-.34-2.787.283-.73.909-1.072 1.674-1.145.477-.045.945.004 1.379.236.57.305.902.77 1.01 1.412.086.512.07 1.012-.075 1.508-.257.878-.888 1.333-1.753 1.448-.718.096-1.446.108-2.17.157-.056.004-.113 0-.178 0z" fill="#027AFF"/><path d="M17.962 1.844h-4.326l-3.425 7.81H5.369V1.878H1.5V22h3.87v-8.477h6.824a3.025 3.025 0 002.743-1.75V22h3.87v-8.477a3.87 3.87 0 00-3.588-3.86v-.01h-2.125a3.94 3.94 0 002.323-2.12l2.545-5.689z" fill="currentColor"/></svg>"##
        }
        Provider::CcMirror => {
            r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z" fill="#f472b6" fill-rule="nonzero"/></svg>"##
        }
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
    let (total_input, total_output, total_cache_read) =
        detail
            .messages
            .iter()
            .fold((0u64, 0u64, 0u64), |(inp, out, cr), msg| {
                if let Some(u) = &msg.token_usage {
                    (
                        inp + u.input_tokens as u64,
                        out + u.output_tokens as u64,
                        cr + u.cache_read_input_tokens as u64,
                    )
                } else {
                    (inp, out, cr)
                }
            });
    let has_tokens = total_input > 0 || total_output > 0;

    let user_svg = user_avatar_svg();
    let assistant_svg = provider_avatar_svg(&detail.meta.provider);

    let mut messages_html = String::new();
    for msg in &detail.messages {
        let ts = msg
            .timestamp
            .as_deref()
            .map(|t| html_escape(&format_msg_ts(t)))
            .unwrap_or_default();
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
                        parts.push(format!(
                            "cache_read {}",
                            fmt_k(u.cache_read_input_tokens as u64)
                        ));
                    }
                    if u.cache_creation_input_tokens > 0 {
                        parts.push(format!(
                            "cache_write {}",
                            fmt_k(u.cache_creation_input_tokens as u64)
                        ));
                    }
                    format!(
                        r#"<div class="msg-token-row">{}</div>"#,
                        html_escape(&parts.join(" · "))
                    )
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

    super::templates::assemble_html(
        &title,
        &provider_label,
        provider_clr,
        &project,
        count,
        &html_escape(&date),
        &messages_html,
        &token_summary_html,
    )
}

pub fn export_html(detail: &SessionDetail, output_path: &Path) -> Result<(), String> {
    let html = super::redact_home_path(&render(detail));
    fs::write(output_path, html).map_err(|e| format!("failed to write file: {e}"))?;
    Ok(())
}
