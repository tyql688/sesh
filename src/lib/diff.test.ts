import { describe, expect, it } from "vitest";

import { buildPatchLineDiff, buildToolLineDiff } from "./diff";

describe("buildToolLineDiff", () => {
  it("renders unchanged lines as context and changed lines as remove/add", () => {
    expect(buildToolLineDiff("a\nold\nc\n", "a\nnew\nc\n")).toEqual([
      { type: "context", oldLine: 1, newLine: 1, text: "a" },
      { type: "remove", oldLine: 2, newLine: null, text: "old" },
      { type: "add", oldLine: null, newLine: 2, text: "new" },
      { type: "context", oldLine: 3, newLine: 3, text: "c" },
    ]);
  });

  it("tracks inserted and deleted line numbers", () => {
    expect(buildToolLineDiff("a\nc\n", "a\nb\nc\n")).toEqual([
      { type: "context", oldLine: 1, newLine: 1, text: "a" },
      { type: "add", oldLine: null, newLine: 2, text: "b" },
      { type: "context", oldLine: 2, newLine: 3, text: "c" },
    ]);

    expect(buildToolLineDiff("a\nb\nc\n", "a\nc\n")).toEqual([
      { type: "context", oldLine: 1, newLine: 1, text: "a" },
      { type: "remove", oldLine: 2, newLine: null, text: "b" },
      { type: "context", oldLine: 3, newLine: 2, text: "c" },
    ]);
  });

  it("collapses very large diffs", () => {
    const oldText = Array.from({ length: 220 }, (_, i) => `old ${i}`).join(
      "\n",
    );
    const newText = Array.from({ length: 220 }, (_, i) => `new ${i}`).join(
      "\n",
    );
    const lines = buildToolLineDiff(oldText, newText, 25);

    expect(lines.length).toBe(25);
    expect(lines.some((line) => line.type === "skip")).toBe(true);
  });
});

describe("buildPatchLineDiff", () => {
  it("converts apply_patch hunks into diff rows", () => {
    const lines = buildPatchLineDiff(`*** Begin Patch
*** Update File: src/app.ts
@@
-const oldValue = 1;
+const newValue = 2;
 const same = true;
*** End Patch
`);

    expect(lines).toEqual([
      {
        type: "skip",
        oldLine: null,
        newLine: null,
        text: "*** Update File: src/app.ts",
      },
      { type: "skip", oldLine: null, newLine: null, text: "@@" },
      {
        type: "remove",
        oldLine: null,
        newLine: null,
        text: "const oldValue = 1;",
      },
      {
        type: "add",
        oldLine: null,
        newLine: null,
        text: "const newValue = 2;",
      },
      {
        type: "context",
        oldLine: null,
        newLine: null,
        text: "const same = true;",
      },
    ]);
  });
});
