import { diffLines } from "diff";
import { shortenHomePath } from "./formatters";

export type ToolDiffLineType = "context" | "add" | "remove" | "skip";

export interface ToolDiffLine {
  type: ToolDiffLineType;
  oldLine: number | null;
  newLine: number | null;
  text: string;
}

interface StructuredPatchHunk {
  oldStart?: number;
  oldLines?: number;
  newStart?: number;
  newLines?: number;
  lines?: unknown;
}

const MAX_VISIBLE_DIFF_LINES = 160;
const EDGE_CONTEXT_LINES = 70;

function stripTrailingNewline(line: string): string {
  return line.endsWith("\n") ? line.slice(0, -1) : line;
}

function pushLine(
  lines: ToolDiffLine[],
  type: Exclude<ToolDiffLineType, "skip">,
  text: string,
  oldLine: number | null,
  newLine: number | null,
) {
  lines.push({
    type,
    oldLine,
    newLine,
    text: stripTrailingNewline(text),
  });
}

export function buildToolLineDiff(
  oldText: string,
  newText: string,
  maxVisibleLines = MAX_VISIBLE_DIFF_LINES,
): ToolDiffLine[] {
  const lines: ToolDiffLine[] = [];
  let oldLine = 1;
  let newLine = 1;

  for (const part of diffLines(oldText, newText)) {
    const rawLines = part.value.match(/[^\n]*\n|[^\n]+/g) ?? [];
    for (const rawLine of rawLines) {
      if (part.added) {
        pushLine(lines, "add", rawLine, null, newLine);
        newLine += 1;
      } else if (part.removed) {
        pushLine(lines, "remove", rawLine, oldLine, null);
        oldLine += 1;
      } else {
        pushLine(lines, "context", rawLine, oldLine, newLine);
        oldLine += 1;
        newLine += 1;
      }
    }
  }

  if (lines.length <= maxVisibleLines) {
    return lines;
  }

  const headCount = Math.min(
    EDGE_CONTEXT_LINES,
    Math.floor(maxVisibleLines / 2),
  );
  const tailCount = Math.min(
    EDGE_CONTEXT_LINES,
    Math.max(0, maxVisibleLines - headCount - 1),
  );
  const hiddenCount = Math.max(0, lines.length - headCount - tailCount);

  return [
    ...lines.slice(0, headCount),
    {
      type: "skip",
      oldLine: null,
      newLine: null,
      text: `${hiddenCount.toLocaleString()} unchanged/changed lines hidden`,
    },
    ...lines.slice(lines.length - tailCount),
  ];
}

export function buildPatchLineDiff(
  patchText: string,
  maxVisibleLines = MAX_VISIBLE_DIFF_LINES,
): ToolDiffLine[] {
  const lines: ToolDiffLine[] = [];

  for (const rawLine of patchText.split("\n")) {
    if (
      rawLine === "*** Begin Patch" ||
      rawLine === "*** End Patch" ||
      rawLine.length === 0
    ) {
      continue;
    }

    if (
      rawLine.startsWith("*** Update File: ") ||
      rawLine.startsWith("*** Add File: ") ||
      rawLine.startsWith("*** Delete File: ") ||
      rawLine.startsWith("*** Move to: ") ||
      rawLine.startsWith("@@")
    ) {
      lines.push({
        type: "skip",
        oldLine: null,
        newLine: null,
        text: shortenHomePath(rawLine),
      });
      continue;
    }

    if (rawLine.startsWith("+")) {
      pushLine(lines, "add", rawLine.slice(1), null, null);
      continue;
    }

    if (rawLine.startsWith("-")) {
      pushLine(lines, "remove", rawLine.slice(1), null, null);
      continue;
    }

    if (rawLine.startsWith(" ")) {
      pushLine(lines, "context", rawLine.slice(1), null, null);
      continue;
    }

    lines.push({
      type: "skip",
      oldLine: null,
      newLine: null,
      text: rawLine,
    });
  }

  if (lines.length <= maxVisibleLines) {
    return lines;
  }

  const headCount = Math.min(
    EDGE_CONTEXT_LINES,
    Math.floor(maxVisibleLines / 2),
  );
  const tailCount = Math.min(
    EDGE_CONTEXT_LINES,
    Math.max(0, maxVisibleLines - headCount - 1),
  );
  const hiddenCount = Math.max(0, lines.length - headCount - tailCount);

  return [
    ...lines.slice(0, headCount),
    {
      type: "skip",
      oldLine: null,
      newLine: null,
      text: `${hiddenCount.toLocaleString()} patch lines hidden`,
    },
    ...lines.slice(lines.length - tailCount),
  ];
}

export function buildStructuredPatchLineDiff(
  structuredPatch: unknown,
  maxVisibleLines = MAX_VISIBLE_DIFF_LINES,
): ToolDiffLine[] {
  if (!Array.isArray(structuredPatch)) {
    return [];
  }

  const lines: ToolDiffLine[] = [];

  for (const hunk of structuredPatch as StructuredPatchHunk[]) {
    if (!hunk || typeof hunk !== "object" || !Array.isArray(hunk.lines)) {
      continue;
    }

    const oldStart =
      typeof hunk.oldStart === "number" && Number.isFinite(hunk.oldStart)
        ? hunk.oldStart
        : null;
    const oldLines =
      typeof hunk.oldLines === "number" && Number.isFinite(hunk.oldLines)
        ? hunk.oldLines
        : 0;
    const newStart =
      typeof hunk.newStart === "number" && Number.isFinite(hunk.newStart)
        ? hunk.newStart
        : null;
    const newLines =
      typeof hunk.newLines === "number" && Number.isFinite(hunk.newLines)
        ? hunk.newLines
        : 0;

    lines.push({
      type: "skip",
      oldLine: null,
      newLine: null,
      text:
        oldStart !== null && newStart !== null
          ? `@@ -${oldStart},${oldLines} +${newStart},${newLines} @@`
          : "@@",
    });

    let oldLine = oldStart;
    let newLine = newStart;
    for (const raw of hunk.lines) {
      if (typeof raw !== "string") {
        continue;
      }

      if (raw.startsWith("+")) {
        pushLine(lines, "add", raw.slice(1), null, newLine);
        if (newLine !== null) newLine += 1;
      } else if (raw.startsWith("-")) {
        pushLine(lines, "remove", raw.slice(1), oldLine, null);
        if (oldLine !== null) oldLine += 1;
      } else if (raw.startsWith(" ")) {
        pushLine(lines, "context", raw.slice(1), oldLine, newLine);
        if (oldLine !== null) oldLine += 1;
        if (newLine !== null) newLine += 1;
      } else {
        lines.push({
          type: "skip",
          oldLine: null,
          newLine: null,
          text: raw,
        });
      }
    }
  }

  if (lines.length <= maxVisibleLines) {
    return lines;
  }

  const headCount = Math.min(
    EDGE_CONTEXT_LINES,
    Math.floor(maxVisibleLines / 2),
  );
  const tailCount = Math.min(
    EDGE_CONTEXT_LINES,
    Math.max(0, maxVisibleLines - headCount - 1),
  );
  const hiddenCount = Math.max(0, lines.length - headCount - tailCount);

  return [
    ...lines.slice(0, headCount),
    {
      type: "skip",
      oldLine: null,
      newLine: null,
      text: `${hiddenCount.toLocaleString()} structured patch lines hidden`,
    },
    ...lines.slice(lines.length - tailCount),
  ];
}
