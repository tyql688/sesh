#[allow(clippy::too_many_arguments)]
pub fn assemble_html(
    title: &str,
    provider_label: &str,
    provider_clr: &str,
    count: u32,
    date: &str,
    file_size: &str,
    messages_html: &str,
    token_summary_html: &str,
    model_html: &str,
    version_html: &str,
    branch_html: &str,
    path_html: &str,
) -> String {
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
:root {{ --bg: #f9fafb; --bg-bubble: #fff; --text: #1a1a1a; --text2: #6b7280; --text3: #9ca3af; --border: #e5e7eb; --code-bg: #f3f4f6; --code-fg: #1a1a1a; --inline-code-bg: rgba(0,0,0,0.06); --inline-code-color: #d63384; --diff-old: rgba(239,68,68,0.12); --diff-new: rgba(34,197,94,0.12); }}
@media (prefers-color-scheme: dark) {{
  :root {{ --bg: #111; --bg-bubble: #1c1c1e; --text: #e5e5e5; --text2: #9ca3af; --text3: #6b7280; --border: #333; --code-bg: #0d0d0d; --code-fg: #cdd6f4; --inline-code-bg: rgba(255,255,255,0.1); --inline-code-color: #f0abfc; --diff-old: rgba(239,68,68,0.15); --diff-new: rgba(34,197,94,0.15); }}
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
.avatar {{ width: 34px; height: 34px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; margin-top: 4px; }}
.avatar-user {{ color: #007aff; }}
.avatar-assistant {{ color: {provider_clr}; }}
.bubble {{ max-width: 85%; padding: 12px 16px; border-radius: 16px; word-wrap: break-word; overflow-wrap: break-word; }}
.bubble-user {{ background: #007aff; color: #fff; border-bottom-right-radius: 4px; }}
.bubble-user .ts, .bubble-user .role-label {{ color: rgba(255,255,255,0.7); }}
.bubble-user a {{ color: #b3d9ff; }}
.bubble-assistant {{ background: var(--bg-bubble); border: 1px solid var(--border); color: var(--text); border-bottom-left-radius: 4px; }}
.msg-header {{ display: flex; align-items: center; margin-bottom: 4px; gap: 8px; }}
.msg-actions {{ margin-left: auto; }}
.role-label {{ font-size: 0.75em; font-weight: 600; color: var(--text2); }}
.copy-btn {{ background: none; border: none; cursor: pointer; font-size: 0.8em; padding: 2px 4px; border-radius: 4px; opacity: 0; transition: opacity 0.15s; }}
.bubble:hover .copy-btn {{ opacity: 0.5; }}
.copy-btn:hover {{ opacity: 1 !important; background: var(--inline-code-bg); }}
.bubble-user .copy-btn {{ color: rgba(255,255,255,0.7); }}
.ts {{ font-size: 0.7em; color: var(--text3); white-space: nowrap; }}
.msg-body {{ font-size: 0.95em; }}
.msg-body > :first-child {{ margin-top: 0; }}
.msg-body > :last-child {{ margin-bottom: 0; }}
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
code {{ background: var(--inline-code-bg); color: var(--inline-code-color); padding: 2px 5px; border-radius: 4px; font-family: 'SF Mono',Menlo,monospace; font-size: 0.85em; }}
.bubble-user code {{ background: rgba(255,255,255,0.15); color: #fce4ec; }}
blockquote {{ border-left: 3px solid var(--border); margin: 8px 0; padding: 2px 12px; color: var(--text2); }}
blockquote p {{ margin: 4px 0; }}
h1, h2, h3, h4 {{ margin: 10px 0 4px; font-weight: 600; }}
h1 {{ font-size: 1.2em; }} h2 {{ font-size: 1.1em; }} h3 {{ font-size: 1.0em; }} h4 {{ font-size: 0.95em; }}
ul, ol {{ margin: 4px 0; padding-left: 22px; }}
li {{ margin: 2px 0; }}
li > p {{ margin: 2px 0; }}
table {{ border-collapse: collapse; margin: 8px 0; font-size: 0.9em; }}
th, td {{ border: 1px solid var(--border); padding: 5px 10px; text-align: left; }}
th {{ background: var(--inline-code-bg); font-weight: 600; }}
a {{ color: #6366f1; text-decoration: none; }}
a:hover {{ text-decoration: underline; }}
hr {{ border: none; border-top: 1px solid var(--border); margin: 12px 0; }}
p {{ margin: 4px 0; }}
.msg-image {{ margin: 8px 0; }}
.msg-image img {{ border-radius: 8px; border: 1px solid var(--border); }}
.msg-token-row {{ padding-left: 44px; font-size: 0.78em; color: var(--text3); font-variant-numeric: tabular-nums; margin-top: -12px; }}
.cache-read {{ color: #10b981; }}
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
/* Lightbox */
.lightbox {{ display: none; position: fixed; inset: 0; background: rgba(0,0,0,0.85); z-index: 9999; justify-content: center; align-items: center; cursor: zoom-out; }}
.lightbox.open {{ display: flex; }}
.lightbox img {{ max-width: 92vw; max-height: 92vh; border-radius: 8px; object-fit: contain; }}
</style>
</head>
<body>
<div class="container">
  <div class="header">
    <h1>{title}</h1>
    <div class="header-meta">
      <span class="badge" style="background:{provider_clr}">{provider_label}</span>
      {path_html}
      <span>💬 {count} messages</span>
      <span>📅 {date}</span>
      <span>📦 {file_size}</span>
      {token_summary_html}
      {model_html}
      {version_html}
      {branch_html}
    </div>
  </div>
  <div class="messages">
{messages_html}
  </div>
</div>
<div class="lightbox" id="lightbox" onclick="closeLightbox()"><img id="lightbox-img" src="" alt="Preview"></div>
<script>
function openLightbox(src){{document.getElementById('lightbox-img').src=src;document.getElementById('lightbox').classList.add('open')}}
function closeLightbox(){{document.getElementById('lightbox').classList.remove('open')}}
document.addEventListener('keydown',function(e){{if(e.key==='Escape')closeLightbox()}})
function copyMsg(id){{var el=document.getElementById(id);if(!el)return;var body=el.querySelector('.msg-body');if(!body)return;navigator.clipboard.writeText(body.innerText).then(function(){{var btn=el.querySelector('.copy-btn');if(btn){{btn.textContent='✅';setTimeout(function(){{btn.textContent='📋'}},1500)}}}})}}
</script>
</body>
</html>"#,
        title = title,
        provider_clr = provider_clr,
        provider_label = provider_label,
        count = count,
        date = date,
        file_size = file_size,
        messages_html = messages_html,
        token_summary_html = token_summary_html,
        model_html = model_html,
        version_html = version_html,
        branch_html = branch_html,
        path_html = path_html,
    )
}
