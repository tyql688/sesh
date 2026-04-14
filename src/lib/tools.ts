import type { Message, ToolMetadata } from "./types";
import type { ToolDiffLine } from "./diff";
import { buildPatchLineDiff, buildStructuredPatchLineDiff } from "./diff";
import { shortenHomePath } from "./formatters";

export interface ToolDetail {
  lines: { label: string; value: string }[];
  diff?: { old: string; new: string };
  patchDiff?: ToolDiffLine[];
  persistedOutputPath?: string;
}

function isPathLabel(label: string): boolean {
  const normalized = label.toLowerCase();
  return (
    normalized === "file" ||
    normalized === "path" ||
    normalized.endsWith("path")
  );
}

function toolLine(
  label: string,
  value: unknown,
): { label: string; value: string } {
  const stringValue = String(value ?? "");
  return {
    label,
    value: isPathLabel(label) ? shortenHomePath(stringValue) : stringValue,
  };
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
      if (line.startsWith("*** Delete File: ")) {
        return line.slice("*** Delete File: ".length).trim();
      }
      if (line.startsWith("*** Move to: ")) {
        return line.slice("*** Move to: ".length).trim();
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
  FollowupTask: "📋",
  ListAgents: "🤖",
  RequestPermissions: "🔐",
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
        return shortenHomePath(
          firstString(obj, ["file_path", "filePath", "path"]),
        );
      case "Bash":
        return firstString(obj, ["description", "command", "cmd"]).slice(0, 80);
      case "Glob":
        return firstString(obj, ["pattern"]);
      case "Grep": {
        const pattern = firstString(obj, ["pattern", "query"]);
        const path = firstString(obj, ["path"]);
        return `/${pattern}/` + (path ? ` ${shortenHomePath(path)}` : "");
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
  } catch (error) {
    console.warn(`Failed to summarize tool input for ${name}:`, error);
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
          const files = extractPatchedFiles(obj.patch);
          return {
            lines: [
              ...(files.length > 0
                ? [
                    {
                      label: "files",
                      value: files.map(shortenHomePath).join("\n"),
                    },
                  ]
                : [toolLine("file", obj.file_path || obj.filePath || "")]),
            ],
            patchDiff: buildPatchLineDiff(obj.patch),
          };
        }
        return {
          lines: [toolLine("file", obj.file_path || obj.filePath || "")],
          diff: {
            old: String(obj.old_string || obj.oldString || ""),
            new: String(obj.new_string || obj.newString || ""),
          },
        };
      case "Write":
        return {
          lines: [
            toolLine("file", obj.file_path || obj.filePath || ""),
            { label: "content", value: String(obj.content || "") },
          ],
        };
      case "Read":
        return {
          lines: [
            toolLine("file", obj.file_path || obj.filePath || ""),
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
            ...(obj.path ? [toolLine("path", obj.path)] : []),
            ...(obj.glob ? [{ label: "glob", value: String(obj.glob) }] : []),
          ],
        };
      default:
        return {
          lines: Object.entries(obj)
            .filter(([, v]) => typeof v === "string" || typeof v === "number")
            .map(([k, v]) => toolLine(k, v))
            .slice(0, 8),
        };
    }
  } catch (error) {
    console.warn(`Failed to format tool input for ${name}:`, error);
    if (
      (name === "Apply_patch" || name === "Edit") &&
      inputJson.includes("*** Begin Patch")
    ) {
      const files = extractPatchedFiles(inputJson);
      return {
        lines: [
          ...(files.length > 0
            ? [{ label: "files", value: files.map(shortenHomePath).join("\n") }]
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

function valueToDisplayString(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number") return value.toLocaleString();
  if (typeof value === "boolean") return value ? "true" : "false";
  if (Array.isArray(value)) {
    return value.map(valueToDisplayString).filter(Boolean).join(", ");
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const from = record.from;
    const to = record.to;
    if (
      (typeof from === "string" || typeof from === "number") &&
      (typeof to === "string" || typeof to === "number")
    ) {
      return `${valueToDisplayString(from)} → ${valueToDisplayString(to)}`;
    }
    return Object.entries(record)
      .map(([key, nested]) => `${key}: ${valueToDisplayString(nested)}`)
      .join(", ");
  }
  return "";
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

function nestedRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function patchFiles(structured: Record<string, unknown>): string[] {
  const files = new Set<string>();
  const pushFiles = (value: unknown) => {
    if (!Array.isArray(value)) return;
    for (const file of value) {
      if (typeof file === "string" && file.length > 0) {
        files.add(shortenHomePath(file));
      }
    }
  };

  const patch = nestedRecord(structured.patch);
  pushFiles(patch?.files);

  if (Array.isArray(structured.patches)) {
    for (const item of structured.patches) {
      pushFiles(nestedRecord(item)?.files);
    }
  }

  return [...files];
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
      {
        const cwd = firstString(structured, ["cwd"]);
        if (cwd) lines.push(toolLine("cwd", cwd));

        const source = firstString(structured, ["source"]);
        if (source) lines.push({ label: "source", value: source });

        const exitCode = maybeNumber(
          structured.exitCode ?? structured.exit_code,
        );
        if (exitCode) lines.push({ label: "exit", value: exitCode });

        const duration = maybeNumber(
          structured.durationSeconds ?? structured.duration_seconds,
        );
        if (duration) lines.push({ label: "duration", value: `${duration}s` });
      }
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
      if (file) lines.push(toolLine("file", file));

      const metadataRecord = nestedRecord(structured.metadata);
      const fileDiffRecord = nestedRecord(metadataRecord?.filediff);

      const patchFilesList = patchFiles(structured);
      if (patchFilesList.length > 0) {
        lines.push({ label: "files", value: patchFilesList.join("\n") });
      }

      const patchText =
        firstString(structured, ["diff"]) ||
        firstString(metadataRecord ?? {}, ["diff"]) ||
        firstString(fileDiffRecord ?? {}, ["patch"]);
      if (patchText) {
        return {
          lines,
          patchDiff: buildPatchLineDiff(patchText),
          persistedOutputPath,
        };
      }

      const structuredPatch = buildStructuredPatchLineDiff(
        structured.structuredPatch,
      );
      if (structuredPatch.length > 0) {
        return {
          lines,
          patchDiff: structuredPatch,
          persistedOutputPath,
        };
      }

      const oldText = firstString(structured, ["oldString", "old_string"]);
      const newText = firstString(structured, ["newString", "new_string"]);
      if (oldText || newText) {
        return {
          lines,
          diff: {
            old: oldText,
            new: newText,
          },
          persistedOutputPath,
        };
      }

      if (
        structured.type === "create" &&
        typeof structured.content === "string" &&
        structured.content.length > 0
      ) {
        return {
          lines,
          diff: {
            old: "",
            new: structured.content,
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
      {
        const nickname = firstString(structured, [
          "nickname",
          "new_agent_nickname",
          "receiver_agent_nickname",
        ]);
        if (nickname) lines.push({ label: "nickname", value: nickname });

        const role = firstString(structured, [
          "new_agent_role",
          "receiver_agent_role",
        ]);
        if (role) lines.push({ label: "role", value: role });

        if (structured.timed_out === true) {
          lines.push({ label: "timedOut", value: "true" });
        }
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
            lines.push({
              label: key,
              value: valueToDisplayString(structured[key]),
            });
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
