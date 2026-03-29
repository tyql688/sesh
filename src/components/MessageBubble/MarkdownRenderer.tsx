import { JSX } from "solid-js";
import katex from "katex";
import { CodeBlock } from "../CodeBlock";
import { MermaidBlock } from "../MermaidBlock";

export interface ContentSegment {
  type: "text" | "code" | "image";
  content: string;
  language?: string;
}

export function parseContent(raw: string): ContentSegment[] {
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
export function renderMarkdownText(text: string, highlightTerm?: string): JSX.Element {
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
