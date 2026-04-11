import { describe, expect, it } from "vitest";

import {
  formatToolInput,
  formatToolResultMetadata,
  parseMcpToolName,
  toolDisplayName,
  toolIcon,
  toolSummary,
} from "./tools";
import type { Message } from "./types";

const baseMessage: Message = {
  role: "tool",
  content: "",
  timestamp: null,
  tool_name: null,
  tool_input: null,
  token_usage: null,
};

describe("tool registry", () => {
  it("parses and displays MCP tool names", () => {
    const name = "mcp__plugin_playwright_playwright__browser_take_screenshot";

    expect(parseMcpToolName(name)).toEqual({
      server: "plugin_playwright_playwright",
      tool: "browser_take_screenshot",
      display: "browser take screenshot",
    });
    expect(toolIcon(name)).toBe("🔌");
    expect(toolDisplayName(name)).toBe("browser take screenshot");
  });

  it("uses Claude metadata summaries before falling back to input JSON", () => {
    const message: Message = {
      ...baseMessage,
      tool_name: "TaskUpdate",
      tool_input: JSON.stringify({ taskId: "1", status: "in_progress" }),
      tool_metadata: {
        raw_name: "TaskUpdate",
        canonical_name: "TaskUpdate",
        display_name: "TaskUpdate",
        category: "task",
        summary: "Fix Live2D leak",
      },
    };

    expect(toolSummary(message)).toBe("Fix Live2D leak");
  });

  it("formats structured edit results as a diff", () => {
    const detail = formatToolResultMetadata({
      raw_name: "Edit",
      canonical_name: "Edit",
      display_name: "Edit",
      category: "file",
      status: "success",
      structured: {
        file_path: "/tmp/App.tsx",
        old_string: "old",
        new_string: "new",
      },
    });

    expect(detail?.lines).toContainEqual({
      label: "file",
      value: "/tmp/App.tsx",
    });
    expect(detail?.diff).toEqual({ old: "old", new: "new" });
  });

  it("formats Claude structuredPatch results as patch diff rows", () => {
    const detail = formatToolResultMetadata({
      raw_name: "Edit",
      canonical_name: "Edit",
      display_name: "Edit",
      category: "file",
      status: "success",
      structured: {
        filePath: "/tmp/App.tsx",
        structuredPatch: [
          {
            oldStart: 4,
            oldLines: 2,
            newStart: 4,
            newLines: 2,
            lines: [" const same = true;", "-old", "+new"],
          },
        ],
      },
    });

    expect(detail?.patchDiff?.map((line) => line.type)).toEqual([
      "skip",
      "context",
      "remove",
      "add",
    ]);
  });

  it("formats canonical input for known tools", () => {
    const detail = formatToolInput({
      ...baseMessage,
      tool_name: "Grep",
      tool_input: JSON.stringify({
        pattern: "fn main",
        path: "/Users/alice/repo/src",
      }),
    });

    expect(
      toolSummary({
        ...baseMessage,
        tool_name: "Grep",
        tool_input: JSON.stringify({
          pattern: "fn main",
          path: "/Users/alice/repo/src",
        }),
      }),
    ).toBe("/fn main/ ~/repo/src");
    expect(detail?.lines).toContainEqual({
      label: "pattern",
      value: "fn main",
    });
    expect(detail?.lines).toContainEqual({
      label: "path",
      value: "~/repo/src",
    });
  });

  it("formats Codex apply_patch input as patch diff rows", () => {
    const detail = formatToolInput({
      ...baseMessage,
      tool_name: "Edit",
      tool_input: JSON.stringify({
        patch: `*** Begin Patch
*** Update File: /Users/alice/project/src/app.ts
@@
-old
+new
*** End Patch
`,
      }),
    });

    expect(detail?.patchDiff?.map((line) => line.type)).toEqual([
      "skip",
      "skip",
      "remove",
      "add",
    ]);
    expect(detail?.lines).toEqual([
      { label: "files", value: "~/project/src/app.ts" },
    ]);
    expect(detail?.patchDiff?.[0]?.text).toBe(
      "*** Update File: ~/project/src/app.ts",
    );
  });
});
