import { describe, expect, it } from "vitest";
import {
  collectFootnotes,
  footnoteDomId,
  headingTagName,
  parseContent,
  parseMarkdownAst,
  sanitizeMessageForClipboard,
} from "./MarkdownRenderer";

describe("parseMarkdownAst", () => {
  it("converts image placeholders into image nodes inside markdown AST", () => {
    const tree = parseMarkdownAst(
      "Before [Image: source: /tmp/screenshot.png] after",
    );

    expect(tree.children).toHaveLength(1);
    expect(tree.children[0]?.type).toBe("paragraph");

    const paragraph = tree.children[0];
    if (!paragraph || paragraph.type !== "paragraph") {
      throw new Error("expected paragraph");
    }

    expect(paragraph.children.map((child) => child.type)).toEqual([
      "text",
      "image",
      "text",
    ]);

    const image = paragraph.children[1];
    if (!image || image.type !== "image") {
      throw new Error("expected image node");
    }

    expect(image.url).toBe("/tmp/screenshot.png");
  });

  it("parses GFM task lists and tables", () => {
    const tree = parseMarkdownAst(
      "- [x] shipped\n- [ ] pending\n\n| Name | Value |\n| :--- | ---: |\n| left | right |",
    );

    expect(tree.children[0]?.type).toBe("list");
    const list = tree.children[0];
    if (!list || list.type !== "list") {
      throw new Error("expected list node");
    }

    expect(list.children.map((item) => item.checked)).toEqual([true, false]);

    expect(tree.children[1]?.type).toBe("table");
    const table = tree.children[1];
    if (!table || table.type !== "table") {
      throw new Error("expected table node");
    }

    expect(table.align).toEqual(["left", "right"]);
  });

  it("parses inline and block math nodes", () => {
    const tree = parseMarkdownAst("Inline $x^2$ here.\n\n$$\ny=x+1\n$$");

    expect(tree.children[0]?.type).toBe("paragraph");
    const paragraph = tree.children[0];
    if (!paragraph || paragraph.type !== "paragraph") {
      throw new Error("expected paragraph");
    }

    expect(paragraph.children.map((child) => child.type)).toContain(
      "inlineMath",
    );

    expect(tree.children[1]?.type).toBe("math");
  });

  it("parses footnote references and definitions", () => {
    const tree = parseMarkdownAst(
      "This sentence has a footnote[^note].\n\n[^note]: Footnote text",
    );

    expect(tree.children[0]?.type).toBe("paragraph");
    const paragraph = tree.children[0];
    if (!paragraph || paragraph.type !== "paragraph") {
      throw new Error("expected paragraph");
    }

    expect(paragraph.children.map((child) => child.type)).toContain(
      "footnoteReference",
    );
    expect(tree.children[1]?.type).toBe("footnoteDefinition");
  });
});

describe("parseContent", () => {
  it("keeps fenced code whitespace while still splitting images", () => {
    const segments = parseContent(
      "```ts\n\nconst value = 1;\n```\n[Image: source: /tmp/diagram.png]",
    );

    expect(segments).toHaveLength(3);
    expect(segments[0]).toMatchObject({
      type: "code",
      language: "ts",
    });
    expect(segments[0]?.content.startsWith("\n")).toBe(true);
    expect(segments[1]).toEqual({
      type: "text",
      content: "\n",
    });
    expect(segments[2]).toEqual({
      type: "image",
      content: "/tmp/diagram.png",
    });
  });
});

describe("sanitizeMessageForClipboard", () => {
  it("normalizes numbered image placeholders", () => {
    expect(
      sanitizeMessageForClipboard(
        "Before [Image #1: source: /tmp/screenshot.png] after [Image #2]",
      ),
    ).toBe("Before [Image] after [Image]");
  });
});

describe("collectFootnotes", () => {
  it("numbers references by first appearance and exposes stable ids", () => {
    const tree = parseMarkdownAst(
      "B[^second] then A[^first].\n\n[^first]: First footnote\n[^second]: Second footnote",
    );
    const footnotes = collectFootnotes(tree);

    expect(footnotes.order).toEqual(["second", "first"]);
    expect(footnotes.numbers.get("second")).toBe(1);
    expect(footnotes.numbers.get("first")).toBe(2);
    expect(footnoteDomId("spec", "second")).toBe("msg-footnote-spec-second");
  });
});

describe("headingTagName", () => {
  it("maps heading depth four through six distinctly", () => {
    expect(headingTagName(4)).toBe("h4");
    expect(headingTagName(5)).toBe("h5");
    expect(headingTagName(6)).toBe("h6");
    expect(headingTagName(9)).toBe("h6");
  });
});
