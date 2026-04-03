import type { TreeNode, SessionRef, Provider } from "../../lib/types";
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

function countSessions(nodes: TreeNode[]): number {
  let n = 0;
  for (const node of nodes) {
    if (node.node_type === "session") n++;
    else n += countSessions(node.children);
  }
  return n;
}

/** Remove sidechain subagents and update counts. */
export function filterOrphanSubagents(tree: TreeNode[]): TreeNode[] {
  function prune(nodes: TreeNode[]): TreeNode[] {
    return nodes
      .map((node) => {
        const children = prune(node.children);
        // Strip sidechain children from session nodes
        const filtered =
          node.node_type === "session"
            ? children.filter((c) => !c.is_sidechain)
            : children;
        return {
          ...node,
          children: filtered,
          count: node.node_type !== "session" ? countSessions(filtered) : 0,
        };
      })
      .filter((node) => {
        // Remove sidechain sessions at project level
        if (node.node_type === "session" && node.is_sidechain) {
          return false;
        }
        // Remove empty non-session containers
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

export function buildSessionRef(
  node: TreeNode,
  parentProjectLabel: string,
): SessionRef {
  return {
    id: node.id,
    provider: (node.provider ?? "claude") as Provider,
    title: node.label,
    project_name: parentProjectLabel,
    is_sidechain: node.is_sidechain ?? false,
  };
}
