import { createSignal, createMemo, createEffect, For, Show, JSX } from "solid-js";
import type { Message, Provider } from "../lib/types";
import { readImageBase64 } from "../lib/tauri";
import { CodeBlock } from "./CodeBlock";
import { MermaidBlock } from "./MermaidBlock";
import { useI18n } from "../i18n/index";

interface ContentSegment {
  type: "text" | "code" | "image";
  content: string;
  language?: string;
}

function parseContent(raw: string): ContentSegment[] {
  if (!raw.includes("```") && !raw.includes("[Image")) {
    return [{ type: "text", content: raw }];
  }

  const segments: ContentSegment[] = [];
  // Match code blocks and image references
  const blockRegex = /```([\w+#.-]*)\n?([\s\S]*?)```|\[Image(?:\s*#\d+)?(?::\s*source:\s*([^\]]+))?\]/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = blockRegex.exec(raw)) !== null) {
    if (match.index > lastIndex) {
      segments.push({ type: "text", content: raw.slice(lastIndex, match.index) });
    }
    if (match[2] !== undefined) {
      // Code block
      segments.push({
        type: "code",
        content: match[2].trim(),
        language: match[1] || undefined,
      });
    } else {
      // Image reference
      const imagePath = match[3]?.trim();
      if (imagePath) {
        segments.push({ type: "image", content: imagePath });
      } else {
        segments.push({ type: "text", content: match[0] });
      }
    }
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < raw.length) {
    segments.push({ type: "text", content: raw.slice(lastIndex) });
  }

  return segments;
}

function isLocalPath(source: string): boolean {
  return (
    !source.startsWith("data:") &&
    !source.startsWith("http://") &&
    !source.startsWith("https://") &&
    !source.startsWith("asset:")
  );
}

/** Inline component that loads a local image via IPC and renders it. */
function LocalImage(props: { path: string; onPreview: (src: string) => void }) {
  const [src, setSrc] = createSignal<string | null>(null);

  createEffect(() => {
    readImageBase64(props.path).then(setSrc).catch((e) => {
      console.warn("failed to load image:", props.path, e);
      setSrc(null);
    });
  });

  return (
    <Show when={src()}>
      <div class="msg-image-wrap">
        <img
          src={src()!}
          alt="Image"
          class="msg-image"
          loading="lazy"
          decoding="async"
          draggable={false}
          onClick={() => props.onPreview(src()!)}
        />
      </div>
    </Show>
  );
}

import katex from "katex";

/** Render a LaTeX math expression using KaTeX. Returns HTML string or null on error. */
function renderKatex(tex: string, displayMode: boolean): string | null {
  try {
    return katex.renderToString(tex, { displayMode, throwOnError: false });
  } catch {
    return null;
  }
}

/** Wrap matching substrings in <mark> tags for search highlighting. */
function wrapHighlight(text: string, term: string): JSX.Element {
  if (!term) return <>{text}</>;
  const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = text.split(new RegExp(`(${escaped})`, "gi"));
  const lowerTerm = term.toLowerCase();
  return <>{parts.map(part =>
    part.toLowerCase() === lowerTerm
      ? <mark class="search-highlight">{part}</mark>
      : <>{part}</>
  )}</>;
}

/** Parse inline markdown formatting within a single line and return JSX elements. */
function renderInlineMarkdown(text: string, highlightTerm?: string): JSX.Element {
  // Process inline formatting: bold, italic, inline code, links, math
  const elements: JSX.Element[] = [];
  // Combined regex for inline elements:
  // 0. inline display math: $$...$$
  // 1. inline code: `code`
  // 2. bold: **text** or __text__
  // 3. italic: *text* or _text_
  // 4. links: [text](url)
  // 5. inline math: $...$
  // Inline formatting: code, bold, italic (word-boundary for _ to avoid snake_case), links, math
  const inlineRegex = /(\$\$(.+?)\$\$)|(`([^`]+)`)|(\*\*(.+?)\*\*|__(.+?)__)|(\*(.+?)\*|(?<!\w)_(.+?)_(?!\w))|(\[([^\]]+)\]\(([^)]+)\))|(\$(.+?)\$)/g;

  let lastIdx = 0;
  let m: RegExpExecArray | null;

  while ((m = inlineRegex.exec(text)) !== null) {
    // Push preceding text (with optional search highlight)
    if (m.index > lastIdx) {
      const preceding = text.slice(lastIdx, m.index);
      elements.push(highlightTerm ? wrapHighlight(preceding, highlightTerm) : <>{preceding}</>);
    }

    if (m[1]) {
      // display math $$...$$
      const html = renderKatex(m[2], true);
      if (html) {
        elements.push(<span class="katex-display-inline" innerHTML={html} />);
      } else {
        elements.push(<code>{m[0]}</code>);
      }
    } else if (m[3]) {
      // inline code — highlight inside code spans too
      elements.push(<code>{highlightTerm ? wrapHighlight(m[4], highlightTerm) : m[4]}</code>);
    } else if (m[5]) {
      // bold
      const boldText = m[6] || m[7];
      elements.push(<strong>{highlightTerm ? wrapHighlight(boldText, highlightTerm) : boldText}</strong>);
    } else if (m[8]) {
      // italic
      const italicText = m[9] || m[10];
      elements.push(<em>{highlightTerm ? wrapHighlight(italicText, highlightTerm) : italicText}</em>);
    } else if (m[11]) {
      // link
      const linkText = m[12];
      const linkUrl = m[13];
      elements.push(
        <a href={linkUrl} target="_blank" rel="noopener noreferrer" onClick={(e) => {
          e.preventDefault();
          window.open(linkUrl, "_blank");
        }}>
          {highlightTerm ? wrapHighlight(linkText, highlightTerm) : linkText}
        </a>
      );
    } else if (m[14]) {
      // inline math $...$
      const html = renderKatex(m[15], false);
      if (html) {
        elements.push(<span class="katex-inline" innerHTML={html} />);
      } else {
        elements.push(<code>{m[0]}</code>);
      }
    }

    lastIdx = m.index + m[0].length;
  }

  // Remaining text
  if (lastIdx < text.length) {
    const remaining = text.slice(lastIdx);
    elements.push(highlightTerm ? wrapHighlight(remaining, highlightTerm) : <>{remaining}</>);
  }

  if (elements.length === 0) {
    return highlightTerm ? wrapHighlight(text, highlightTerm) : <>{text}</>;
  }

  return <>{elements}</>;
}

/** Render a text segment with markdown formatting as JSX. */
function renderMarkdownText(text: string, highlightTerm?: string): JSX.Element {
  const lines = text.split("\n");
  const elements: JSX.Element[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trimStart();

    // Display math block: $$...$$ spanning multiple lines
    if (trimmed === "$$") {
      const mathLines: string[] = [];
      i++;
      while (i < lines.length && lines[i].trimStart() !== "$$") {
        mathLines.push(lines[i]);
        i++;
      }
      if (i < lines.length) i++; // skip closing $$
      const tex = mathLines.join("\n");
      const html = renderKatex(tex, true);
      if (html) {
        elements.push(<div class="katex-display-block" innerHTML={html} />);
      } else {
        elements.push(<pre class="code-block-pre"><code>{`$$\n${tex}\n$$`}</code></pre>);
      }
      continue;
    }

    // Headers
    if (trimmed.startsWith("### ")) {
      elements.push(<h3>{renderInlineMarkdown(trimmed.slice(4), highlightTerm)}</h3>);
      i++;
      continue;
    }
    if (trimmed.startsWith("## ")) {
      elements.push(<h2>{renderInlineMarkdown(trimmed.slice(3), highlightTerm)}</h2>);
      i++;
      continue;
    }
    if (trimmed.startsWith("# ")) {
      elements.push(<h1>{renderInlineMarkdown(trimmed.slice(2), highlightTerm)}</h1>);
      i++;
      continue;
    }

    // Horizontal rule: --- or *** or ___
    if (/^[-*_]{3,}\s*$/.test(trimmed)) {
      elements.push(<hr class="msg-hr" />);
      i++;
      continue;
    }

    // Blockquote: > text
    if (trimmed.startsWith("> ")) {
      const quoteLines: string[] = [];
      while (i < lines.length) {
        const ql = lines[i].trimStart();
        if (ql.startsWith("> ")) {
          quoteLines.push(ql.slice(2));
          i++;
        } else if (ql === ">") {
          quoteLines.push("");
          i++;
        } else {
          break;
        }
      }
      elements.push(
        <blockquote class="msg-blockquote">
          {quoteLines.map((ql) => ql ? <p class="msg-text-line">{renderInlineMarkdown(ql, highlightTerm)}</p> : <br />)}
        </blockquote>
      );
      continue;
    }

    // Table: | col | col |
    if (trimmed.startsWith("|") && trimmed.endsWith("|") && trimmed.includes("|", 1)) {
      const tableRows: string[][] = [];
      let hasHeader = false;
      while (i < lines.length) {
        const tl = lines[i].trim();
        if (tl.startsWith("|") && tl.endsWith("|")) {
          const cells = tl.slice(1, -1).split("|").map((c) => c.trim());
          // Skip separator row (| --- | --- |)
          if (cells.every((c) => /^[-:]+$/.test(c))) {
            hasHeader = true;
            i++;
            continue;
          }
          tableRows.push(cells);
          i++;
        } else {
          break;
        }
      }
      if (tableRows.length > 0) {
        const headerRow = hasHeader ? tableRows[0] : null;
        const bodyRows = hasHeader ? tableRows.slice(1) : tableRows;
        elements.push(
          <table class="msg-table">
            {headerRow && (
              <thead>
                <tr>{headerRow.map((c) => <th>{renderInlineMarkdown(c, highlightTerm)}</th>)}</tr>
              </thead>
            )}
            <tbody>
              {bodyRows.map((row) => (
                <tr>{row.map((c) => <td>{renderInlineMarkdown(c, highlightTerm)}</td>)}</tr>
              ))}
            </tbody>
          </table>
        );
      }
      continue;
    }

    // Unordered list items: - item or * item
    if (/^[-*]\s+/.test(trimmed)) {
      const listItems: JSX.Element[] = [];
      while (i < lines.length) {
        const li = lines[i].trimStart();
        const ulMatch = li.match(/^[-*]\s+(.*)/);
        if (ulMatch) {
          listItems.push(<li>{renderInlineMarkdown(ulMatch[1], highlightTerm)}</li>);
          i++;
        } else {
          break;
        }
      }
      elements.push(<ul>{listItems}</ul>);
      continue;
    }

    // Ordered list items: 1. item
    if (/^\d+\.\s+/.test(trimmed)) {
      const listItems: JSX.Element[] = [];
      while (i < lines.length) {
        const li = lines[i].trimStart();
        const olMatch = li.match(/^\d+\.\s+(.*)/);
        if (olMatch) {
          listItems.push(<li>{renderInlineMarkdown(olMatch[1], highlightTerm)}</li>);
          i++;
        } else {
          break;
        }
      }
      elements.push(<ol>{listItems}</ol>);
      continue;
    }

    // Empty line = line break
    if (trimmed === "") {
      elements.push(<br />);
      i++;
      continue;
    }

    // Normal paragraph line
    elements.push(<p class="msg-text-line">{renderInlineMarkdown(line, highlightTerm)}</p>);
    i++;
  }

  return <div class="msg-text">{elements}</div>;
}

export function ProviderIcon(props: { provider: Provider }) {
  switch (props.provider) {
    case "claude":
      return (
        <svg width="14" height="14" fill="currentColor" viewBox="0 0 24 24">
          <path d="M17.3041 3.541h-3.6718l6.696 16.918H24Zm-10.6082 0L0 20.459h3.7442l1.3693-3.5527h7.0052l1.3693 3.5528h3.7442L10.5363 3.5409Zm-.3712 10.2232 2.2914-5.9456 2.2914 5.9456Z" />
        </svg>
      );
    case "codex":
      return (
        <svg width="14" height="14" fill="currentColor" viewBox="0 0 24 24">
          <path d="M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z" />
        </svg>
      );
    case "gemini":
      return (
        <svg width="14" height="14" fill="currentColor" viewBox="0 0 24 24">
          <path d="M11.04 19.32Q12 21.51 12 24q0-2.49.93-4.68.96-2.19 2.58-3.81t3.81-2.55Q21.51 12 24 12q-2.49 0-4.68-.93a12.3 12.3 0 0 1-3.81-2.58 12.3 12.3 0 0 1-2.58-3.81Q12 2.49 12 0q0 2.49-.96 4.68-.93 2.19-2.55 3.81a12.3 12.3 0 0 1-3.81 2.58Q2.49 12 0 12q2.49 0 4.68.96 2.19.93 3.81 2.55t2.55 3.81" />
        </svg>
      );
    case "cursor":
      return (
        <svg width="14" height="14" fill="currentColor" viewBox="0 0 24 24">
          <path d="M11.503.131 1.891 5.678a.84.84 0 0 0-.42.726v11.188c0 .3.162.575.42.724l9.609 5.55a1 1 0 0 0 .998 0l9.61-5.55a.84.84 0 0 0 .42-.724V6.404a.84.84 0 0 0-.42-.726L12.497.131a1.01 1.01 0 0 0-.996 0M2.657 6.338h18.55c.263 0 .43.287.297.515L12.23 22.918c-.062.107-.229.064-.229-.06V12.335a.59.59 0 0 0-.295-.51l-9.11-5.257c-.109-.063-.064-.23.061-.23" />
        </svg>
      );
    case "opencode":
      return (
        <svg width="14" height="14" fill="currentColor" viewBox="0 0 24 24">
          <path fill-rule="evenodd" clip-rule="evenodd" d="M18 19.5H6V4.5H18V19.5ZM15 7.5H9V16.5H15V7.5Z" />
        </svg>
      );
    default:
      return <span>A</span>;
  }
}

function UserIcon() {
  return (
    <svg width="14" height="14" fill="currentColor" viewBox="0 0 24 24">
      <path d="M12 12c2.7 0 4.8-2.1 4.8-4.8S14.7 2.4 12 2.4 7.2 4.5 7.2 7.2 9.3 12 12 12zm0 2.4c-3.2 0-9.6 1.6-9.6 4.8v2.4h19.2v-2.4c0-3.2-6.4-4.8-9.6-4.8z" />
    </svg>
  );
}

function ImagePreview(props: { src: string; onClose: () => void }) {
  return (
    <div class="image-preview-overlay" onClick={props.onClose}>
      <img src={props.src} class="image-preview-img" onClick={(e) => e.stopPropagation()} />
      <button class="image-preview-close" aria-label="Close preview" onClick={props.onClose}>
        <svg width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
          <line x1="18" y1="6" x2="6" y2="18" /><line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>
    </div>
  );
}

function CopyMessageButton(props: { content: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = createSignal(false);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(props.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // clipboard unavailable
    }
  }

  return (
    <button
      class="msg-copy-btn"
      onClick={handleCopy}
      title={t("common.copyMessage")}
      aria-label={t("common.copyMessage")}
    >
      {copied() ? (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      ) : (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
          <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
          <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
        </svg>
      )}
    </button>
  );
}

function TokenUsageDisplay(props: { usage: import("../lib/types").TokenUsage }) {
  const fmt = (n: number) => n.toLocaleString();
  const cached = () => props.usage.cache_read_input_tokens;
  const created = () => props.usage.cache_creation_input_tokens;
  return (
    <div class="msg-token-usage">
      <span title="Input tokens">↑{fmt(props.usage.input_tokens)}</span>
      <span class="msg-token-sep">·</span>
      <span title="Output tokens">↓{fmt(props.usage.output_tokens)}</span>
      <Show when={cached() > 0}>
        <span class="msg-token-sep">·</span>
        <span class="msg-token-cached" title="Cache read tokens">cache_read {fmt(cached())}</span>
      </Show>
      <Show when={created() > 0}>
        <span class="msg-token-sep">·</span>
        <span class="msg-token-cache-write" title="Cache creation tokens">cache_write {fmt(created())}</span>
      </Show>
    </div>
  );
}

export function MessageBubble(props: { message: Message; provider?: Provider; highlightTerm?: string }) {
  const segments = createMemo(() => parseContent(props.message.content));
  const [previewSrc, setPreviewSrc] = createSignal<string | null>(null);

  const isEmpty = (): boolean => {
    const msg = props.message;
    if (msg.role === "tool") {
      // Hide tool_result entries (toolu_ IDs from Anthropic API)
      if (msg.tool_name?.startsWith("toolu_")) return true;
      return !msg.content && !msg.tool_input && !msg.tool_name;
    }
    return !msg.content || msg.content.trim().length === 0;
  };

  const isSystemContent = (): boolean => {
    const msg = props.message;
    if (msg.role === "tool") return false;
    if (!msg.content || msg.content.trim().length === 0) return false;
    const c = msg.content.trimStart();
    // Skip known system/template content markers
    const systemMarkers = [
      "</observation>", "</command-message>", "<INSTRUCTIONS>",
      "<environment_context>", "<permissions instructions>",
      "</facts>", "</narrative>", "</concepts>",
      "<system-reminder>",
    ];
    return systemMarkers.some((marker) => c.includes(marker));
  };

  if (isEmpty() || isSystemContent()) return null;

  return (
    <>
    <Show
      when={props.message.role !== "tool"}
      fallback={<ToolMessage message={props.message} />}
    >
      <Show
        when={props.message.role !== "system"}
        fallback={
          props.message.content.startsWith("[thinking]\n")
            ? <ThinkingBlock content={props.message.content.slice("[thinking]\n".length)} />
            : <div class="msg-system">{props.message.content}</div>
        }
      >
        <div class={`msg-row msg-row-${props.message.role}`}>
          <div class={`msg-avatar msg-avatar-${props.message.role}${props.message.role === "assistant" ? ` ${props.provider ?? "claude"}` : ""}`}>
            <Show
              when={props.message.role === "user"}
              fallback={<ProviderIcon provider={props.provider ?? "claude"} />}
            >
              <UserIcon />
            </Show>
          </div>
          <div class={`msg-bubble msg-bubble-${props.message.role}`}>
            <For each={segments()}>
              {(seg) => {
                if (seg.type === "code") {
                  if (seg.language?.toLowerCase() === "mermaid") {
                    return <MermaidBlock code={seg.content} />;
                  }
                  return <CodeBlock code={seg.content} language={seg.language} />;
                }
                if (seg.type === "image") {
                  if (isLocalPath(seg.content)) {
                    return (
                      <LocalImage path={seg.content} onPreview={(s) => setPreviewSrc(s)} />
                    );
                  }
                  return (
                    <div class="msg-image-wrap">
                      <img
                        src={seg.content}
                        alt="Image"
                        class="msg-image"
                        loading="lazy"
                        decoding="async"
                        draggable={false}
                        onClick={() => setPreviewSrc(seg.content)}
                      />
                    </div>
                  );
                }
                // createMemo makes this reactive: re-renders when highlightTerm signal changes.
                // <For> callbacks only run once per item, so without memo the highlight would be static.
                return createMemo(() => renderMarkdownText(seg.content, props.highlightTerm)) as unknown as JSX.Element;
              }}
            </For>
            <CopyMessageButton content={props.message.content} />
          </div>
        </div>
        <Show when={props.message.role === "assistant" && props.message.token_usage}>
          <div class="msg-token-row">
            <TokenUsageDisplay usage={props.message.token_usage!} />
          </div>
        </Show>
      </Show>
    </Show>
    <Show when={previewSrc()}>
      <ImagePreview src={previewSrc()!} onClose={() => setPreviewSrc(null)} />
    </Show>
    </>
  );
}

function shortPath(p: string): string {
  return p?.split("/").slice(-2).join("/") || "";
}

/** Extract a human-readable summary from tool input JSON. */
function toolSummary(name: string, inputJson: string): string {
  try {
    const obj = JSON.parse(inputJson);
    switch (name) {
      case "Read": return shortPath(obj.file_path);
      case "Edit": return shortPath(obj.file_path);
      case "Write": return shortPath(obj.file_path);
      case "Bash": return obj.description || obj.command?.slice(0, 60) || "";
      case "Glob": return obj.pattern || "";
      case "Grep": return `/${obj.pattern}/` + (obj.path ? ` ${shortPath(obj.path)}` : "");
      case "Agent": return obj.description || "";
      default: {
        const first = Object.values(obj).find((v) => typeof v === "string" && (v as string).length > 0);
        return first ? String(first).slice(0, 60) : "";
      }
    }
  } catch {
    return "";
  }
}

/** Format tool input for expanded view — structured, not raw JSON. */
function formatToolInput(name: string, inputJson: string): { lines: { label: string; value: string }[]; diff?: { old: string; new: string } } {
  try {
    const obj = JSON.parse(inputJson);
    switch (name) {
      case "Edit":
        return {
          lines: [{ label: "file", value: obj.file_path || "" }],
          diff: { old: obj.old_string || "", new: obj.new_string || "" },
        };
      case "Write":
        return {
          lines: [
            { label: "file", value: obj.file_path || "" },
            { label: "content", value: obj.content || "" },
          ],
        };
      case "Read":
        return {
          lines: [
            { label: "file", value: obj.file_path || "" },
            ...(obj.offset ? [{ label: "offset", value: String(obj.offset) }] : []),
            ...(obj.limit ? [{ label: "limit", value: String(obj.limit) }] : []),
          ],
        };
      case "Bash":
        return { lines: [{ label: "command", value: obj.command || obj.cmd || "" }] };
      case "Plan": {
        const lines: { label: string; value: string }[] = [];
        if (obj.explanation) lines.push({ label: "explanation", value: obj.explanation });
        if (Array.isArray(obj.plan)) {
          const planText = obj.plan.map((s: { step: string; status: string }) => {
            const icon = s.status === "completed" ? "✓" : s.status === "in_progress" ? "▸" : "○";
            return `${icon} ${s.step}`;
          }).join("\n");
          lines.push({ label: "plan", value: planText });
        }
        return { lines };
      }
      case "Grep":
        return {
          lines: [
            { label: "pattern", value: obj.pattern || "" },
            ...(obj.path ? [{ label: "path", value: obj.path }] : []),
            ...(obj.glob ? [{ label: "glob", value: obj.glob }] : []),
          ],
        };
      default:
        return { lines: Object.entries(obj).filter(([, v]) => typeof v === "string" || typeof v === "number").map(([k, v]) => ({ label: k, value: String(v) })).slice(0, 5) };
    }
  } catch {
    // apply_patch: raw patch text, extract file path from header
    if (name === "Apply_patch" && inputJson.includes("*** Begin Patch")) {
      const fileMatch = inputJson.match(/\*\*\* (?:Add|Update|Delete) File:\s*(.+)/);
      const filePath = fileMatch ? fileMatch[1].trim() : "";
      return { lines: [
        ...(filePath ? [{ label: "file", value: filePath }] : []),
        { label: "patch", value: inputJson },
      ] };
    }
    return { lines: [{ label: "raw", value: inputJson }] };
  }
}

const TOOL_ICONS: Record<string, string> = {
  Read: "📄", Edit: "✏️", Apply_patch: "✏️", Plan: "📋", Write: "📝", Bash: "⬛", Glob: "🔍",
  Grep: "🔎", Agent: "🤖", WebSearch: "🌐", WebFetch: "🌐",
  TaskCreate: "📋", TaskUpdate: "📋", Skill: "⚡", mcp: "🔌",
};

/** Parse MCP tool name: mcp__server__tool → { server, tool, display } */
function parseMcpToolName(name: string): { server: string; tool: string; display: string } | null {
  if (!name.startsWith("mcp__")) return null;
  const parts = name.slice(5).split("__");
  if (parts.length < 2) return null;
  const tool = parts.slice(1).join("__");
  return { server: parts[0], tool, display: tool.replace(/_/g, " ") };
}

/** Get display name for a tool (handles MCP names). */
export function formatMcpLabel(name: string): string {
  const mcp = parseMcpToolName(name);
  return mcp ? mcp.display : name;
}

function toolDisplayName(name: string): string {
  const mcp = parseMcpToolName(name);
  return mcp ? mcp.display : name;
}

/** Get icon for a tool (handles MCP names). */
function toolIcon(name: string): string {
  if (name.startsWith("mcp__")) return TOOL_ICONS.mcp;
  return TOOL_ICONS[name] || "⚙";
}

function ThinkingBlock(props: { content: string }) {
  const [expanded, setExpanded] = createSignal(false);
  const preview = () => {
    const first = props.content.split("\n")[0];
    return first.length > 80 ? first.slice(0, 80) + "..." : first;
  };

  return (
    <div class={`msg-thinking${expanded() ? " expanded" : ""}`}>
      <div class="msg-thinking-header" onClick={() => setExpanded(!expanded())}>
        <span class="msg-thinking-icon">💭</span>
        <span class="msg-thinking-label">Thinking</span>
        <Show when={!expanded()}>
          <span class="msg-thinking-preview">{preview()}</span>
        </Show>
        <span class="msg-thinking-chevron">{expanded() ? "▾" : "▸"}</span>
      </div>
      <Show when={expanded()}>
        <pre class="msg-thinking-content">{props.content}</pre>
      </Show>
    </div>
  );
}

function ToolMessage(props: { message: Message }) {
  const [expanded, setExpanded] = createSignal(false);
  const [previewSrc, setPreviewSrc] = createSignal<string | null>(null);

  const hasInput = () => !!props.message.tool_input && props.message.tool_input.trim().length > 0;
  const hasOutput = () => !!props.message.content && props.message.content.trim().length > 0;
  const hasName = () => !!props.message.tool_name && props.message.tool_name.trim().length > 0;

  if (!hasName()) return null;

  const name = () => props.message.tool_name || "";
  const mcp = () => parseMcpToolName(name());
  const icon = () => toolIcon(name());
  const displayName = () => toolDisplayName(name());
  const summary = createMemo(() =>
    hasInput() ? toolSummary(name(), props.message.tool_input!) : ""
  );
  const formatted = createMemo(() =>
    hasInput() ? formatToolInput(name(), props.message.tool_input!) : null
  );

  return (
    <div class={`msg-tool${expanded() ? " expanded" : ""}`}>
      <div class="msg-tool-header" onClick={() => setExpanded(!expanded())}>
        <span class="msg-tool-icon">{icon()}</span>
        <span class="msg-tool-name">{displayName()}</span>
        <Show when={mcp()}>
          <span class="msg-tool-server">{mcp()!.server}</span>
        </Show>
        <Show when={summary()}>
          <span class="msg-tool-summary">{summary()}</span>
        </Show>
        <Show when={hasInput() || hasOutput()}>
          <span class="tool-expand-indicator">
            {expanded() ? "▾" : "▸"}
          </span>
        </Show>
      </div>
      <Show when={expanded()}>
        <Show when={formatted()}>
          <div class="msg-tool-detail">
            <For each={formatted()!.lines}>
              {(line) => (
                <div class="msg-tool-field">
                  <span class="msg-tool-field-label">{line.label}</span>
                  <pre class="msg-tool-field-value">{line.value}</pre>
                </div>
              )}
            </For>
            <Show when={formatted()!.diff}>
              <div class="msg-tool-diff">
                <div class="msg-tool-diff-old">
                  <span class="msg-tool-diff-label">-</span>
                  <pre>{formatted()!.diff!.old}</pre>
                </div>
                <div class="msg-tool-diff-new">
                  <span class="msg-tool-diff-label">+</span>
                  <pre>{formatted()!.diff!.new}</pre>
                </div>
              </div>
            </Show>
          </div>
        </Show>
        <Show when={!formatted() && hasInput()}>
          <pre class="msg-tool-input">{props.message.tool_input!}</pre>
        </Show>
        <Show when={hasOutput()}>
          <div class="msg-tool-output">
            <For each={parseContent(props.message.content)}>
              {(seg) => {
                if (seg.type === "image") {
                  return (
                    <div class="msg-image-wrap">
                      <img
                        src={seg.content}
                        alt="Tool output"
                        class="msg-image"
                        loading="lazy"
                        decoding="async"
                        onClick={() => setPreviewSrc(seg.content)}
                      />
                    </div>
                  );
                }
                return <pre>{seg.content}</pre>;
              }}
            </For>
          </div>
        </Show>
      </Show>
      <Show when={previewSrc()}>
        <ImagePreview src={previewSrc()!} onClose={() => setPreviewSrc(null)} />
      </Show>
    </div>
  );
}
