import { describe, it, expect } from "vitest";
import { buildFavoritesTree, buildTrashTree } from "./tree-builders";
import type { SessionMeta, TrashMeta } from "./types";

function makeSession(overrides: Partial<SessionMeta> = {}): SessionMeta {
  return {
    id: "sess-1",
    provider: "claude",
    title: "Test Session",
    project_path: "/home/user/project",
    project_name: "project",
    created_at: 1711800000,
    updated_at: 1711800000,
    message_count: 5,
    file_size_bytes: 1024,
    source_path: "/home/user/.claude/projects/project/session.jsonl",
    is_sidechain: false,
    ...overrides,
  };
}

function makeTrashItem(overrides: Partial<TrashMeta> = {}): TrashMeta {
  return {
    id: "trash-1",
    provider: "claude",
    title: "Trashed Session",
    original_path: "/home/user/.claude/projects/myproject/session.jsonl",
    trashed_at: 1711800000,
    trash_file: "/trash/trash-1.jsonl",
    ...overrides,
  };
}

describe("buildFavoritesTree", () => {
  it("returns [] for empty input", () => {
    expect(buildFavoritesTree([], "No Project")).toEqual([]);
  });

  it("groups by provider then project", () => {
    const sessions = [
      makeSession({ id: "s1", provider: "claude", project_name: "proj-a", project_path: "/a" }),
      makeSession({ id: "s2", provider: "claude", project_name: "proj-a", project_path: "/a" }),
      makeSession({ id: "s3", provider: "codex", project_name: "proj-b", project_path: "/b" }),
    ];
    const tree = buildFavoritesTree(sessions, "No Project");

    expect(tree).toHaveLength(2);

    const claudeNode = tree.find((n) => n.provider === "claude");
    expect(claudeNode).toBeDefined();
    expect(claudeNode!.node_type).toBe("provider");
    expect(claudeNode!.count).toBe(2);
    expect(claudeNode!.children).toHaveLength(1);
    expect(claudeNode!.children[0].node_type).toBe("project");
    expect(claudeNode!.children[0].children).toHaveLength(2);

    const codexNode = tree.find((n) => n.provider === "codex");
    expect(codexNode).toBeDefined();
    expect(codexNode!.count).toBe(1);
  });
});

describe("buildTrashTree", () => {
  const labels = { unknown: "Unknown", untitled: "Untitled" };

  it("returns [] for empty input", () => {
    expect(buildTrashTree([], labels)).toEqual([]);
  });

  it("derives project from original_path", () => {
    const items = [
      makeTrashItem({
        id: "t1",
        original_path: "/home/user/.claude/projects/myproject/session.jsonl",
      }),
    ];
    const tree = buildTrashTree(items, labels);

    expect(tree).toHaveLength(1);
    expect(tree[0].node_type).toBe("provider");
    expect(tree[0].children).toHaveLength(1);
    // Project derived from second-to-last path segment
    expect(tree[0].children[0].label).toBe("myproject");
    expect(tree[0].children[0].children).toHaveLength(1);
    expect(tree[0].children[0].children[0].id).toBe("t1");
  });
});
