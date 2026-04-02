import { createSignal, createMemo, Show, For } from "solid-js";
import type { Message } from "../../lib/types";
import { parseContent } from "./MarkdownRenderer";
import { ImagePreview } from "./ImagePreview";

function shortPath(p: string): string {
  return p?.split("/").slice(-2).join("/") || "";
}

function extractPatchedFiles(patchText: string): string[] {
  const files = patchText
    .split("\n")
    .map((line) => {
      if (line.startsWith("*** Update File: ")) {
        return line.slice("*** Update File: ".length).trim();
      }
      if (line.startsWith("*** Add File: ")) {
        return line.slice("*** Add File: ".length).trim();
      }
      return "";
    })
    .filter((s) => s.length > 0);
  return [...new Set(files)];
}

/** Extract a human-readable summary from tool input JSON. */
function toolSummary(name: string, inputJson: string): string {
  try {
    const obj = JSON.parse(inputJson);
    switch (name) {
      case "Read":
        return shortPath(obj.file_path);
      case "Edit":
        return shortPath(obj.file_path);
      case "Write":
        return shortPath(obj.file_path);
      case "Bash":
        return obj.description || obj.command?.slice(0, 60) || "";
      case "Glob":
        return obj.pattern || "";
      case "Grep":
        return `/${obj.pattern}/` + (obj.path ? ` ${shortPath(obj.path)}` : "");
      case "Agent":
        return obj.description || "";
      default: {
        const first = Object.values(obj).find(
          (v) => typeof v === "string" && (v as string).length > 0,
        );
        return first ? String(first).slice(0, 60) : "";
      }
    }
  } catch {
    // Kimi CLI may truncate parallel Agent ToolCall arguments.
    // Try to extract description from partial JSON like: {"description":"xxx","
    if (name === "Agent") {
      const m = inputJson.match(/"description"\s*:\s*"([^"]+)"/);
      if (m) return m[1];
    }
    return "";
  }
}

/** Format tool input for expanded view — structured, not raw JSON. */
function formatToolInput(
  name: string,
  inputJson: string,
): {
  lines: { label: string; value: string }[];
  diff?: { old: string; new: string };
} {
  try {
    const obj = JSON.parse(inputJson);
    switch (name) {
      case "Edit":
        if (obj.patch) {
          return {
            lines: [
              { label: "file", value: obj.file_path || "" },
              { label: "patch", value: obj.patch },
            ],
          };
        }
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
            ...(obj.offset
              ? [{ label: "offset", value: String(obj.offset) }]
              : []),
            ...(obj.limit
              ? [{ label: "limit", value: String(obj.limit) }]
              : []),
          ],
        };
      case "Bash":
        return {
          lines: [{ label: "command", value: obj.command || obj.cmd || "" }],
        };
      case "Plan": {
        const lines: { label: string; value: string }[] = [];
        if (obj.explanation)
          lines.push({ label: "explanation", value: obj.explanation });
        if (Array.isArray(obj.plan)) {
          const planText = obj.plan
            .map((s: { step: string; status: string }) => {
              const icon =
                s.status === "completed"
                  ? "✓"
                  : s.status === "in_progress"
                    ? "▸"
                    : "○";
              return `${icon} ${s.step}`;
            })
            .join("\n");
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
        return {
          lines: Object.entries(obj)
            .filter(([, v]) => typeof v === "string" || typeof v === "number")
            .map(([k, v]) => ({ label: k, value: String(v) }))
            .slice(0, 5),
        };
    }
  } catch {
    if (name === "Apply_patch" && inputJson.includes("*** Begin Patch")) {
      const files = extractPatchedFiles(inputJson);
      return {
        lines: [
          ...(files.length > 0
            ? [{ label: "files", value: files.join("\n") }]
            : []),
          { label: "patch", value: inputJson },
        ],
      };
    }
    return { lines: [{ label: "raw", value: inputJson }] };
  }
}

const TOOL_ICONS: Record<string, string> = {
  Read: "📄",
  Edit: "✏️",
  Apply_patch: "✏️",
  Plan: "📋",
  Write: "📝",
  Bash: "💻",
  Glob: "🔍",
  Grep: "🔎",
  Agent: "🤖",
  WebSearch: "🌐",
  WebFetch: "🌐",
  TaskCreate: "📋",
  TaskUpdate: "📋",
  Skill: "⚡",
  mcp: "🔌",
};

/** Parse MCP tool name: mcp__server__tool → { server, tool, display } */
export function parseMcpToolName(
  name: string,
): { server: string; tool: string; display: string } | null {
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

/** Dispatch a custom event to open a subagent session by description, nickname, or agent ID. */
function openSubagent(
  description: string,
  nickname?: string,
  agentId?: string,
) {
  window.dispatchEvent(
    new CustomEvent("open-subagent", {
      detail: { description, nickname, agentId },
    }),
  );
}

export function ToolMessage(props: { message: Message }) {
  const [expanded, setExpanded] = createSignal(false);
  const [previewSrc, setPreviewSrc] = createSignal<string | null>(null);

  const hasInput = () =>
    !!props.message.tool_input && props.message.tool_input.trim().length > 0;
  const hasOutput = () =>
    !!props.message.content && props.message.content.trim().length > 0;
  const hasName = () =>
    !!props.message.tool_name && props.message.tool_name.trim().length > 0;

  if (!hasName()) return null;

  const name = () => props.message.tool_name || "";
  const mcp = () => parseMcpToolName(name());
  const icon = () => toolIcon(name());
  const displayName = () => toolDisplayName(name());
  const summary = createMemo(() =>
    hasInput() ? toolSummary(name(), props.message.tool_input!) : "",
  );
  const formatted = createMemo(() =>
    hasInput() ? formatToolInput(name(), props.message.tool_input!) : null,
  );
  /** Extract nickname from Agent tool output (Codex: {"nickname":"Faraday"}) */
  const agentNickname = createMemo(() => {
    if (name() !== "Agent" || !hasOutput()) return undefined;
    try {
      const obj = JSON.parse(props.message.content);
      return obj.nickname as string | undefined;
    } catch {
      return undefined;
    }
  });
  /** Extract agent_id from Agent tool output (Kimi: "agent_id: xxx\n...") */
  const agentId = createMemo(() => {
    if (name() !== "Agent" || !hasOutput()) return undefined;
    const m = props.message.content.match(/^agent_id:\s*(\S+)/m);
    return m ? m[1] : undefined;
  });

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
        <Show
          when={
            name() === "Agent" && (summary() || agentNickname() || agentId())
          }
        >
          <button
            class="msg-tool-subagent-link"
            onClick={(e) => {
              e.stopPropagation();
              openSubagent(summary(), agentNickname(), agentId());
            }}
            title="Open subagent session"
          >
            ↗ Open
          </button>
        </Show>
        <Show when={hasInput() || hasOutput()}>
          <span class="tool-expand-indicator">{expanded() ? "▾" : "▸"}</span>
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
