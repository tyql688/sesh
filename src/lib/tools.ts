import type { Message, ToolMetadata } from "./types";
import type { ToolDiffLine } from "./diff";
import { buildPatchLineDiff } from "./diff";

export interface ToolDetail {
  lines: { label: string; value: string }[];
  diff?: { old: string; new: string };
  patchDiff?: ToolDiffLine[];
  persistedOutputPath?: string;
}

export function shortPath(p: string): string {
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
  TaskList: "📋",
  TaskStop: "🛑",
  ToolSearch: "🧰",
  Skill: "⚡",
  AskUserQuestion: "❓",
  CronCreate: "⏰",
  CronDelete: "⏰",
  EnterPlanMode: "🧭",
  ExitPlanMode: "🧭",
  SendMessage: "✉️",
  ListMcpResourcesTool: "🔌",
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

export function formatMcpLabel(name: string): string {
  const mcp = parseMcpToolName(name);
  return mcp ? mcp.display : name;
}

export function toolDisplayName(name: string, metadata?: ToolMetadata): string {
  if (metadata?.display_name) return metadata.display_name;
  return formatMcpLabel(name);
}

export function toolIcon(name: string, metadata?: ToolMetadata): string {
  if (metadata?.category === "mcp" || name.startsWith("mcp__")) {
    return TOOL_ICONS.mcp;
  }
  return (
    TOOL_ICONS[metadata?.canonical_name ?? name] || TOOL_ICONS[name] || "⚙"
  );
}

function firstString(obj: Record<string, unknown>, keys: string[]): string {
  for (const key of keys) {
    const value = obj[key];
    if (typeof value === "string" && value.length > 0) return value;
  }
  return "";
}

/** Extract a human-readable summary from tool input JSON. */
export function toolSummary(message: Message): string {
  const name = message.tool_name || "";
  if (message.tool_metadata?.summary) return message.tool_metadata.summary;
  const inputJson = message.tool_input;
  if (!inputJson) return "";

  try {
    const obj = JSON.parse(inputJson) as Record<string, unknown>;
    switch (name) {
      case "Read":
      case "Edit":
      case "Write":
        return shortPath(firstString(obj, ["file_path", "filePath", "path"]));
      case "Bash":
        return firstString(obj, ["description", "command", "cmd"]).slice(0, 80);
      case "Glob":
        return firstString(obj, ["pattern"]);
      case "Grep": {
        const pattern = firstString(obj, ["pattern", "query"]);
        const path = firstString(obj, ["path"]);
        return `/${pattern}/` + (path ? ` ${shortPath(path)}` : "");
      }
      case "Agent":
        return firstString(obj, ["description", "prompt"]);
      case "Skill":
        return firstString(obj, ["skill"]);
      case "ToolSearch":
      case "WebSearch":
        return firstString(obj, ["query"]);
      case "WebFetch":
        return firstString(obj, ["url"]);
      default: {
        const first = Object.values(obj).find(
          (v) => typeof v === "string" && (v as string).length > 0,
        );
        return first ? String(first).slice(0, 80) : "";
      }
    }
  } catch {
    if (name === "Agent") {
      const m = inputJson.match(/"description"\s*:\s*"([^"]+)"/);
      if (m) return m[1];
    }
    return "";
  }
}

/** Format tool input for expanded view — structured, not raw JSON. */
export function formatToolInput(message: Message): ToolDetail | null {
  const name = message.tool_name || "";
  const inputJson = message.tool_input;
  if (!inputJson) return null;

  try {
    const obj = JSON.parse(inputJson) as Record<string, unknown>;
    switch (name) {
      case "Edit":
        if (typeof obj.patch === "string") {
          return {
            lines: [
              {
                label: "file",
                value: String(obj.file_path || obj.filePath || ""),
              },
            ],
            patchDiff: buildPatchLineDiff(obj.patch),
          };
        }
        return {
          lines: [
            {
              label: "file",
              value: String(obj.file_path || obj.filePath || ""),
            },
          ],
          diff: {
            old: String(obj.old_string || obj.oldString || ""),
            new: String(obj.new_string || obj.newString || ""),
          },
        };
      case "Write":
        return {
          lines: [
            {
              label: "file",
              value: String(obj.file_path || obj.filePath || ""),
            },
            { label: "content", value: String(obj.content || "") },
          ],
        };
      case "Read":
        return {
          lines: [
            {
              label: "file",
              value: String(obj.file_path || obj.filePath || ""),
            },
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
          lines: [
            { label: "command", value: String(obj.command || obj.cmd || "") },
          ],
        };
      case "Plan": {
        const lines: { label: string; value: string }[] = [];
        if (typeof obj.explanation === "string") {
          lines.push({ label: "explanation", value: obj.explanation });
        }
        if (Array.isArray(obj.plan)) {
          const planText = obj.plan
            .map((s) => {
              if (!s || typeof s !== "object") return "";
              const step = "step" in s ? String(s.step) : "";
              const status = "status" in s ? String(s.status) : "";
              const icon =
                status === "completed"
                  ? "✓"
                  : status === "in_progress"
                    ? "▸"
                    : "○";
              return `${icon} ${step}`;
            })
            .filter(Boolean)
            .join("\n");
          lines.push({ label: "plan", value: planText });
        }
        return { lines };
      }
      case "Grep":
        return {
          lines: [
            { label: "pattern", value: String(obj.pattern || obj.query || "") },
            ...(obj.path ? [{ label: "path", value: String(obj.path) }] : []),
            ...(obj.glob ? [{ label: "glob", value: String(obj.glob) }] : []),
          ],
        };
      default:
        return {
          lines: Object.entries(obj)
            .filter(([, v]) => typeof v === "string" || typeof v === "number")
            .map(([k, v]) => ({ label: k, value: String(v) }))
            .slice(0, 8),
        };
    }
  } catch {
    if (
      (name === "Apply_patch" || name === "Edit") &&
      inputJson.includes("*** Begin Patch")
    ) {
      const files = extractPatchedFiles(inputJson);
      return {
        lines: [
          ...(files.length > 0
            ? [{ label: "files", value: files.join("\n") }]
            : []),
        ],
        patchDiff: buildPatchLineDiff(inputJson),
      };
    }
    return { lines: [{ label: "raw", value: inputJson }] };
  }
}

function maybeNumber(value: unknown): string | undefined {
  return typeof value === "number" ? value.toLocaleString() : undefined;
}

function structuredRecord(
  metadata: ToolMetadata | undefined,
): Record<string, unknown> | null {
  const structured = metadata?.structured;
  return structured &&
    typeof structured === "object" &&
    !Array.isArray(structured)
    ? (structured as Record<string, unknown>)
    : null;
}

export function formatToolResultMetadata(
  metadata?: ToolMetadata,
): ToolDetail | null {
  const structured = structuredRecord(metadata);
  if (!metadata || !structured) return null;

  const lines: { label: string; value: string }[] = [];
  const persistedOutputPath =
    typeof structured.persistedOutputPath === "string"
      ? structured.persistedOutputPath
      : undefined;

  if (metadata.status) lines.push({ label: "status", value: metadata.status });

  switch (metadata.canonical_name) {
    case "Bash":
      if (
        typeof structured.stdout === "string" &&
        structured.stdout.length > 0
      ) {
        lines.push({ label: "stdout", value: structured.stdout });
      }
      if (
        typeof structured.stderr === "string" &&
        structured.stderr.length > 0
      ) {
        lines.push({ label: "stderr", value: structured.stderr });
      }
      break;
    case "Edit":
    case "Write": {
      const file = firstString(structured, ["filePath", "file_path"]);
      if (file) lines.push({ label: "file", value: file });
      if (
        typeof structured.oldString === "string" ||
        typeof structured.newString === "string"
      ) {
        return {
          lines,
          diff: {
            old: String(structured.oldString || ""),
            new: String(structured.newString || ""),
          },
          persistedOutputPath,
        };
      }
      break;
    }
    case "Agent":
      for (const [label, key] of [
        ["agent", "agentId"],
        ["type", "agentType"],
        ["tokens", "totalTokens"],
        ["tools", "totalToolUseCount"],
      ] as const) {
        const value =
          typeof structured[key] === "string"
            ? structured[key]
            : maybeNumber(structured[key]);
        if (value) lines.push({ label, value });
      }
      break;
    case "ToolSearch":
      lines.push(
        { label: "query", value: String(structured.query || "") },
        {
          label: "matches",
          value: Array.isArray(structured.matches)
            ? String(structured.matches.length)
            : String(structured.total_deferred_tools || ""),
        },
      );
      break;
    case "WebFetch":
      for (const key of ["url", "code", "codeText", "durationMs"]) {
        if (structured[key] !== undefined) {
          lines.push({ label: key, value: String(structured[key]) });
        }
      }
      break;
    default:
      if (metadata.category === "task") {
        for (const key of ["taskId", "task_id", "statusChange", "message"]) {
          if (structured[key] !== undefined) {
            lines.push({ label: key, value: String(structured[key]) });
          }
        }
      } else if (metadata.category === "mcp" && metadata.mcp) {
        lines.push(
          { label: "server", value: metadata.mcp.server },
          { label: "tool", value: metadata.mcp.tool },
        );
      }
  }

  return lines.length > 0 || persistedOutputPath
    ? { lines, persistedOutputPath }
    : null;
}
