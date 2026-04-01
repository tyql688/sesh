import type { TreeNode, SessionMeta, Provider } from "../../lib/types";
import { isPathBlocked } from "../../stores/settings";

/** Filter out projects whose path matches a blocked folder. */
export function filterBlockedFolders(tree: TreeNode[]): TreeNode[] {
  return tree
    .map((provider) => ({
      ...provider,
      children: provider.children.filter((project) => {
        const path = project.project_path ?? "";
        return !path || !isPathBlocked(path);
      }),
    }))
    .filter((provider) => provider.children.length > 0);
}

/** Remove orphan subagents (is_sidechain=true without parent) and prune empty containers. */
export function filterOrphanSubagents(tree: TreeNode[]): TreeNode[] {
  function prune(nodes: TreeNode[]): TreeNode[] {
    return nodes
      .map((node) => ({
        ...node,
        children: prune(node.children),
      }))
      .filter((node) => {
        // Remove orphan sidechain sessions (no children of their own)
        if (
          node.node_type === "session" &&
          node.is_sidechain &&
          node.children.length === 0
        ) {
          return false;
        }
        // Remove empty non-session containers (projects/providers with no children left)
        if (node.node_type !== "session" && node.children.length === 0) {
          return false;
        }
        return true;
      });
  }
  return prune(tree);
}

export function applyTimeGrouping(
  tree: TreeNode[],
  t: (key: string) => string,
): TreeNode[] {
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const weekStart = new Date(todayStart);
  const dayOfWeek = weekStart.getDay();
  weekStart.setDate(
    weekStart.getDate() - (dayOfWeek === 0 ? 6 : dayOfWeek - 1),
  );
  const monthStart = new Date(todayStart);
  monthStart.setDate(1);

  const todayMs = todayStart.getTime();
  const weekMs = weekStart.getTime();
  const monthMs = monthStart.getTime();

  function groupLabel(epochSec: number): string {
    const ms = epochSec * 1000;
    if (ms >= todayMs) return t("explorer.today");
    if (ms >= weekMs) return t("explorer.thisWeek");
    if (ms >= monthMs) return t("explorer.thisMonth");
    return t("explorer.older");
  }

  return tree.map((provider) => ({
    ...provider,
    children: provider.children.map((project) => {
      if (project.children.length <= 3) return project; // no grouping for small projects
      const groups = new Map<string, TreeNode[]>();
      for (const session of project.children) {
        const label = groupLabel(session.updated_at || 0);
        if (!groups.has(label)) groups.set(label, []);
        groups.get(label)!.push(session);
      }
      if (groups.size <= 1) return project; // all in one group, no benefit
      const groupNodes: TreeNode[] = [];
      for (const [label, sessions] of groups) {
        groupNodes.push({
          id: `${project.id}:${label}`,
          label,
          node_type: "project",
          children: sessions,
          count: sessions.length,
          provider: project.provider,
        });
      }
      return { ...project, children: groupNodes };
    }),
  }));
}

export function buildSessionMeta(
  node: TreeNode,
  parentProjectLabel: string,
): SessionMeta {
  return {
    id: node.id,
    provider: (node.provider ?? "claude") as Provider,
    title: node.label,
    project_path: "",
    project_name: parentProjectLabel,
    created_at: 0,
    updated_at: 0,
    message_count: 0,
    file_size_bytes: 0,
    source_path: "",
    is_sidechain: node.is_sidechain ?? false,
  };
}
