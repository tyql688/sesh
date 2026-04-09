import { JSX, For } from "solid-js";
import katex from "katex";
import type {
  BlockContent,
  Code,
  Definition,
  FootnoteDefinition,
  FootnoteReference,
  Heading,
  Image,
  ImageReference,
  Link,
  LinkReference,
  List,
  ListItem,
  Paragraph,
  PhrasingContent,
  Root,
  RootContent,
  Table,
  TableCell,
  Text,
} from "mdast";
import { unified } from "unified";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import remarkParse from "remark-parse";
import { visit } from "unist-util-visit";
import { CodeBlock } from "../CodeBlock";
import { MermaidBlock } from "../MermaidBlock";
import { LocalImage, RemoteImage, isLocalPath } from "./ImagePreview";

interface MathNode {
  type: "math";
  value: string;
}

interface InlineMathNode {
  type: "inlineMath";
  value: string;
}

export interface ContentSegment {
  type: "text" | "code" | "image";
  content: string;
  language?: string;
}

type MarkdownBlockNode = RootContent | BlockContent;
type MarkdownInlineNode = PhrasingContent;

interface RenderContext {
  definitions: Map<string, Definition>;
  footnoteDefinitions: Map<string, FootnoteDefinition>;
  footnoteOrder: string[];
  footnoteNumbers: Map<string, number>;
  footnotePrefix: string;
  highlightTerm?: string;
  onPreview: (src: string, source: string) => void;
}

const IMAGE_PLACEHOLDER_REGEX =
  /\[Image(?:\s*#\d+)?(?::\s*source:\s*([^\]]+))?\]/g;

const markdownParser = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkMath);

export function sanitizeMessageForClipboard(raw: string): string {
  return raw.replace(
    /\[Image(?:\s*#\d+)?(?::\s*source:\s*[^\]]+)?\]/g,
    "[Image]",
  );
}

export function parseContent(raw: string): ContentSegment[] {
  if (!raw.includes("```") && !raw.includes("[Image")) {
    return [{ type: "text", content: raw }];
  }

  const segments: ContentSegment[] = [];
  const blockRegex =
    /```([\w+#.-]*)\n?([\s\S]*?)```|\[Image(?:\s*#\d+)?(?::\s*source:\s*([^\]]+))?\]/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = blockRegex.exec(raw)) !== null) {
    if (match.index > lastIndex) {
      segments.push({
        type: "text",
        content: raw.slice(lastIndex, match.index),
      });
    }

    if (match[2] !== undefined) {
      segments.push({
        type: "code",
        content: match[2],
        language: match[1] || undefined,
      });
    } else {
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

export function parseMarkdownAst(raw: string): Root {
  const tree = markdownParser.parse(raw) as Root;
  transformImagePlaceholders(tree);
  return tree;
}

export function renderMarkdownContent(
  raw: string,
  options: {
    footnotePrefix: string;
    highlightTerm?: string;
    onPreview: (src: string, source: string) => void;
  },
): JSX.Element {
  const tree = parseMarkdownAst(raw);
  const footnotes = collectFootnotes(tree);
  const context: RenderContext = {
    definitions: collectDefinitions(tree),
    footnoteDefinitions: footnotes.definitions,
    footnoteOrder: footnotes.order,
    footnoteNumbers: footnotes.numbers,
    footnotePrefix: options.footnotePrefix,
    highlightTerm: options.highlightTerm,
    onPreview: options.onPreview,
  };

  return (
    <div class="msg-text">
      {renderBlockNodes(tree.children, context)}
      {renderFootnotesSection(context)}
    </div>
  );
}

function collectDefinitions(tree: Root): Map<string, Definition> {
  const definitions = new Map<string, Definition>();

  visit(tree, "definition", (node: Definition) => {
    definitions.set(normalizeIdentifier(node.identifier), node);
  });

  return definitions;
}

function normalizeIdentifier(identifier: string): string {
  return identifier.trim().toLowerCase();
}

export function collectFootnotes(tree: Root) {
  const definitions = new Map<string, FootnoteDefinition>();
  const order: string[] = [];
  const seen = new Set<string>();

  visit(tree, "footnoteDefinition", (node: FootnoteDefinition) => {
    definitions.set(normalizeIdentifier(node.identifier), node);
  });

  visit(tree, "footnoteReference", (node: FootnoteReference) => {
    const identifier = normalizeIdentifier(node.identifier);
    if (seen.has(identifier)) return;
    seen.add(identifier);
    order.push(identifier);
  });

  for (const identifier of definitions.keys()) {
    if (seen.has(identifier)) continue;
    seen.add(identifier);
    order.push(identifier);
  }

  return {
    definitions,
    order,
    numbers: new Map(order.map((identifier, index) => [identifier, index + 1])),
  };
}

export function headingTagName(
  depth: number,
): "h1" | "h2" | "h3" | "h4" | "h5" | "h6" {
  if (depth <= 1) return "h1";
  if (depth === 2) return "h2";
  if (depth === 3) return "h3";
  if (depth === 4) return "h4";
  if (depth === 5) return "h5";
  return "h6";
}

function transformImagePlaceholders(tree: Root) {
  visit(tree, "text", (node: Text, index, parent) => {
    if (index === undefined || !parent || !("children" in parent)) {
      return;
    }

    const replacement = splitTextWithImages(node.value);
    if (
      replacement.length === 1 &&
      replacement[0].type === "text" &&
      replacement[0].value === node.value
    ) {
      return;
    }

    parent.children.splice(index, 1, ...replacement);
    return index + replacement.length;
  });
}

function splitTextWithImages(value: string): Array<Text | Image> {
  if (!value.includes("[Image")) {
    return [{ type: "text", value }];
  }

  const nodes: Array<Text | Image> = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  IMAGE_PLACEHOLDER_REGEX.lastIndex = 0;

  while ((match = IMAGE_PLACEHOLDER_REGEX.exec(value)) !== null) {
    if (match.index > lastIndex) {
      nodes.push({ type: "text", value: value.slice(lastIndex, match.index) });
    }

    const imagePath = match[1]?.trim();
    if (imagePath) {
      nodes.push({
        type: "image",
        alt: "Image",
        title: null,
        url: imagePath,
      });
    } else {
      nodes.push({ type: "text", value: match[0] });
    }

    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < value.length) {
    nodes.push({ type: "text", value: value.slice(lastIndex) });
  }

  return nodes;
}

function renderKatex(tex: string, displayMode: boolean): string | null {
  try {
    return katex.renderToString(tex, { displayMode, throwOnError: false });
  } catch {
    return null;
  }
}

function wrapHighlight(text: string, term?: string): JSX.Element {
  if (!term) return <>{text}</>;
  const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const parts = text.split(new RegExp(`(${escaped})`, "gi"));
  const lowerTerm = term.toLowerCase();

  return (
    <>
      <For each={parts}>
        {(part) =>
          part.toLowerCase() === lowerTerm ? (
            <mark class="search-highlight">{part}</mark>
          ) : (
            <>{part}</>
          )
        }
      </For>
    </>
  );
}

function isSafeUrl(url: string): boolean {
  if (url.startsWith("/")) return true;
  try {
    const parsed = new URL(url, "https://placeholder");
    return ["http:", "https:", "mailto:"].includes(parsed.protocol);
  } catch {
    return false;
  }
}

function renderBlockNodes(
  nodes: MarkdownBlockNode[],
  context: RenderContext,
): JSX.Element {
  return (
    <For each={nodes}>
      {(node, index) => renderBlockNode(node, context, `block-${index()}`)}
    </For>
  );
}

function renderBlockNode(
  node: MarkdownBlockNode,
  context: RenderContext,
  key: string,
): JSX.Element | null {
  switch (node.type) {
    case "paragraph":
      return renderParagraph(node, context, key);
    case "heading":
      return renderHeading(node, context, key);
    case "blockquote":
      return (
        <blockquote class="msg-blockquote">
          {renderBlockNodes(node.children, context)}
        </blockquote>
      );
    case "list":
      return renderList(node, context, key);
    case "listItem":
      return renderListItem(node, context, key);
    case "table":
      return renderTable(node, context, key);
    case "code":
      return renderCodeBlock(node, key);
    case "math":
      return renderMathBlock(node, key);
    case "thematicBreak":
      return <hr class="msg-hr" />;
    case "html":
      return (
        <p class="msg-text-line">
          {wrapHighlight(node.value, context.highlightTerm)}
        </p>
      );
    case "definition":
    case "footnoteDefinition":
      return null;
    default:
      return null;
  }
}

function renderParagraph(
  node: Paragraph,
  context: RenderContext,
  _key: string,
): JSX.Element {
  const segments = splitParagraphChildren(node.children);

  if (segments.length === 1 && segments[0].type === "phrasing") {
    return (
      <p class="msg-text-line">
        {renderInlineNodes(segments[0].children, context)}
      </p>
    );
  }

  return (
    <>
      <For each={segments}>
        {(segment) =>
          segment.type === "phrasing" ? (
            <p class="msg-text-line">
              {renderInlineNodes(segment.children, context)}
            </p>
          ) : (
            <div>{renderImageNode(segment.node, context)}</div>
          )
        }
      </For>
    </>
  );
}

function splitParagraphChildren(children: PhrasingContent[]) {
  const segments: Array<
    | { type: "phrasing"; children: PhrasingContent[] }
    | { type: "image"; node: Image }
  > = [];
  let current: PhrasingContent[] = [];

  for (const child of children) {
    if (child.type === "image") {
      if (current.length > 0) {
        segments.push({ type: "phrasing", children: current });
        current = [];
      }
      segments.push({ type: "image", node: child });
    } else {
      current.push(child);
    }
  }

  if (current.length > 0 || segments.length === 0) {
    segments.push({ type: "phrasing", children: current });
  }

  return segments;
}

function renderHeading(
  node: Heading,
  context: RenderContext,
  _key: string,
): JSX.Element {
  const content = renderInlineNodes(node.children, context);

  switch (headingTagName(node.depth)) {
    case "h1":
      return <h1>{content}</h1>;
    case "h2":
      return <h2>{content}</h2>;
    case "h3":
      return <h3>{content}</h3>;
    case "h4":
      return <h4>{content}</h4>;
    case "h5":
      return <h5>{content}</h5>;
    case "h6":
      return <h6>{content}</h6>;
  }
}

function renderList(
  node: List,
  context: RenderContext,
  key: string,
): JSX.Element {
  if (node.ordered) {
    return (
      <ol start={node.start ?? 1}>
        <For each={node.children}>
          {(child, index) =>
            renderListItem(child, context, `${key}-${index()}`)
          }
        </For>
      </ol>
    );
  }

  return (
    <ul>
      <For each={node.children}>
        {(child, index) => renderListItem(child, context, `${key}-${index()}`)}
      </For>
    </ul>
  );
}

function renderListItem(
  node: ListItem,
  context: RenderContext,
  key: string,
): JSX.Element {
  const isTask = typeof node.checked === "boolean";
  const onlyParagraph =
    node.children.length === 1 && node.children[0]?.type === "paragraph";

  const content = onlyParagraph
    ? renderListItemParagraph(node.children[0] as Paragraph, context, key)
    : renderBlockNodes(node.children, context);

  return (
    <li class={isTask ? "msg-task-item" : undefined}>
      {isTask && (
        <input
          class="msg-task-checkbox"
          type="checkbox"
          checked={node.checked === true}
          disabled
        />
      )}
      <div class={isTask ? "msg-task-content" : undefined}>{content}</div>
    </li>
  );
}

function renderListItemParagraph(
  node: Paragraph,
  context: RenderContext,
  _key: string,
): JSX.Element {
  const segments = splitParagraphChildren(node.children);

  if (segments.length === 1 && segments[0].type === "phrasing") {
    return <>{renderInlineNodes(segments[0].children, context)}</>;
  }

  return (
    <>
      <For each={segments}>
        {(segment) =>
          segment.type === "phrasing" ? (
            <p class="msg-text-line">
              {renderInlineNodes(segment.children, context)}
            </p>
          ) : (
            <div>{renderImageNode(segment.node, context)}</div>
          )
        }
      </For>
    </>
  );
}

function renderTable(
  node: Table,
  context: RenderContext,
  _key: string,
): JSX.Element {
  const headerRow = node.children[0];
  const bodyRows = node.children.slice(1);

  return (
    <table class="msg-table">
      <thead>
        <tr>
          <For each={headerRow.children}>
            {(cell, index) =>
              renderTableCell(
                cell,
                node.align?.[index()] ?? null,
                context,
                "th",
              )
            }
          </For>
        </tr>
      </thead>
      <tbody>
        <For each={bodyRows}>
          {(row) => (
            <tr>
              <For each={row.children}>
                {(cell, index) =>
                  renderTableCell(
                    cell,
                    node.align?.[index()] ?? null,
                    context,
                    "td",
                  )
                }
              </For>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  );
}

function renderTableCell(
  node: TableCell,
  align: "left" | "right" | "center" | null | undefined,
  context: RenderContext,
  tag: "th" | "td",
): JSX.Element {
  const content = renderInlineNodes(node.children, context);
  const style = align ? { "text-align": align } : undefined;

  return tag === "th" ? (
    <th style={style}>{content}</th>
  ) : (
    <td style={style}>{content}</td>
  );
}

function renderCodeBlock(node: Code, _key: string): JSX.Element {
  if (node.lang?.toLowerCase() === "mermaid") {
    return <MermaidBlock code={node.value} />;
  }

  return <CodeBlock code={node.value} language={node.lang ?? undefined} />;
}

function renderMathBlock(node: MathNode, _key: string): JSX.Element {
  const html = renderKatex(node.value, true);

  if (html) {
    // eslint-disable-next-line solid/no-innerhtml
    return <div class="katex-display-block" innerHTML={html} />;
  }

  return (
    <pre class="code-block-pre">
      <code>{`$$\n${node.value}\n$$`}</code>
    </pre>
  );
}

function renderInlineNodes(
  nodes: MarkdownInlineNode[],
  context: RenderContext,
): JSX.Element {
  return (
    <For each={nodes}>
      {(node, index) => renderInlineNode(node, context, `inline-${index()}`)}
    </For>
  );
}

function renderInlineNode(
  node: MarkdownInlineNode,
  context: RenderContext,
  key: string,
): JSX.Element | null {
  switch (node.type) {
    case "text":
      return <>{wrapHighlight(node.value, context.highlightTerm)}</>;
    case "strong":
      return <strong>{renderInlineNodes(node.children, context)}</strong>;
    case "emphasis":
      return <em>{renderInlineNodes(node.children, context)}</em>;
    case "delete":
      return <del>{renderInlineNodes(node.children, context)}</del>;
    case "inlineCode":
      return <code>{wrapHighlight(node.value, context.highlightTerm)}</code>;
    case "link":
      return renderLinkNode(node, context, key);
    case "linkReference":
      return renderLinkReferenceNode(node, context, key);
    case "image":
      return renderImageNode(node, context);
    case "imageReference":
      return renderImageReferenceNode(node, context, key);
    case "footnoteReference":
      return renderFootnoteReferenceNode(node, context);
    case "inlineMath":
      return renderInlineMathNode(node, key);
    case "break":
      return <br />;
    case "html":
      return <span>{wrapHighlight(node.value, context.highlightTerm)}</span>;
    default:
      return null;
  }
}

function renderLinkNode(
  node: Link,
  context: RenderContext,
  _key: string,
): JSX.Element {
  if (!isSafeUrl(node.url)) {
    return <span>{renderInlineNodes(node.children, context)}</span>;
  }

  return (
    <a
      href={node.url}
      rel="noopener noreferrer"
      target="_blank"
      title={node.title ?? undefined}
      onClick={(event) => {
        event.preventDefault();
        window.open(node.url, "_blank");
      }}
    >
      {renderInlineNodes(node.children, context)}
    </a>
  );
}

function renderLinkReferenceNode(
  node: LinkReference,
  context: RenderContext,
  _key: string,
): JSX.Element {
  const definition = context.definitions.get(
    normalizeIdentifier(node.identifier),
  );
  if (!definition || !isSafeUrl(definition.url)) {
    return <span>{renderInlineNodes(node.children, context)}</span>;
  }

  return (
    <a
      href={definition.url}
      rel="noopener noreferrer"
      target="_blank"
      title={definition.title ?? undefined}
      onClick={(event) => {
        event.preventDefault();
        window.open(definition.url, "_blank");
      }}
    >
      {renderInlineNodes(node.children, context)}
    </a>
  );
}

function renderImageReferenceNode(
  node: ImageReference,
  context: RenderContext,
  _key: string,
): JSX.Element | null {
  const definition = context.definitions.get(
    normalizeIdentifier(node.identifier),
  );
  if (!definition) {
    return node.alt ? <span>{node.alt}</span> : null;
  }

  return renderImageNode(
    {
      type: "image",
      alt: node.alt,
      title: definition.title,
      url: definition.url,
    },
    context,
  );
}

function renderImageNode(node: Image, context: RenderContext): JSX.Element {
  if (isLocalPath(node.url)) {
    return (
      <LocalImage
        path={node.url}
        onPreview={(src, source) => context.onPreview(src, source)}
      />
    );
  }

  return (
    <RemoteImage
      src={node.url}
      onPreview={(src, source) => context.onPreview(src, source)}
    />
  );
}

export function footnoteDomId(prefix: string, identifier: string): string {
  const normalized = identifier
    .replace(/[^a-z0-9_-]+/gi, "-")
    .replace(/^-+|-+$/g, "");
  return `msg-footnote-${prefix}-${normalized || "note"}`;
}

function renderFootnoteReferenceNode(
  node: FootnoteReference,
  context: RenderContext,
): JSX.Element {
  const identifier = normalizeIdentifier(node.identifier);
  const label = String(
    context.footnoteNumbers.get(identifier) ?? node.label ?? node.identifier,
  );
  const target = context.footnoteDefinitions.has(identifier)
    ? `#${footnoteDomId(context.footnotePrefix, identifier)}`
    : undefined;

  return (
    <sup class="msg-footnote-ref">
      {target ? <a href={target}>{label}</a> : <span>{label}</span>}
    </sup>
  );
}

function renderFootnotesSection(context: RenderContext): JSX.Element | null {
  const footnotes = context.footnoteOrder
    .map((identifier) => ({
      identifier,
      node: context.footnoteDefinitions.get(identifier),
    }))
    .filter(
      (
        entry,
      ): entry is {
        identifier: string;
        node: FootnoteDefinition;
      } => !!entry.node,
    );

  if (footnotes.length === 0) {
    return null;
  }

  return (
    <section class="msg-footnotes">
      <ol>
        <For each={footnotes}>
          {(entry) => (
            <li
              id={footnoteDomId(context.footnotePrefix, entry.identifier)}
              class="msg-footnote-item"
            >
              {renderBlockNodes(entry.node.children, context)}
            </li>
          )}
        </For>
      </ol>
    </section>
  );
}

function renderInlineMathNode(node: InlineMathNode, _key: string): JSX.Element {
  const html = renderKatex(node.value, false);

  if (html) {
    // eslint-disable-next-line solid/no-innerhtml
    return <span class="katex-inline" innerHTML={html} />;
  }

  return <code>{`$${node.value}$`}</code>;
}
