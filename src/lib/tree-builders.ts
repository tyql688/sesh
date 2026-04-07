import type { SessionMeta, TrashMeta, TreeNode, Provider } from "./types";
import { getDisplayLabel } from "./provider-registry";

export function buildFavoritesTree(
  sessions: SessionMeta[],
  noProjectLabel: string,
): TreeNode[] {
  const providerMap = new Map<
    string,
    Map<string, { label: string; sessions: SessionMeta[] }>
  >();

  for (const s of sessions) {
    const provider = s.provider || "claude";
    const projectKey = s.project_path || "__no_project__";
    const projectLabel = s.project_name || noProjectLabel;
    if (!providerMap.has(provider)) {
      providerMap.set(provider, new Map());
    }
    const projectMap = providerMap.get(provider)!;
    if (!projectMap.has(projectKey)) {
      projectMap.set(projectKey, { label: projectLabel, sessions: [] });
    }
    projectMap.get(projectKey)!.sessions.push(s);
  }

  const tree: TreeNode[] = [];
  for (const [provider, projectMap] of providerMap) {
    const projectNodes: TreeNode[] = [];
    for (const [projectKey, group] of projectMap) {
      const sessionNodes: TreeNode[] = group.sessions.map((s) => ({
        id: s.id,
        label: s.title,
        node_type: "session" as const,
        children: [],
        count: 0,
        provider: s.provider as Provider,
      }));
      projectNodes.push({
        id: `fav-${provider}-${projectKey}`,
        label: group.label,
        node_type: "project" as const,
        children: sessionNodes,
        count: sessionNodes.length,
        provider: null,
      });
    }
    tree.push({
      id: `fav-${provider}`,
      label: getDisplayLabel(provider as Provider),
      node_type: "provider" as const,
      children: projectNodes,
      count: sessions.filter((s) => s.provider === provider).length,
      provider: provider as Provider,
    });
  }

  return tree;
}

export function buildTrashTree(
  items: TrashMeta[],
  labels: { unknown: string; untitled: string },
): TreeNode[] {
  const providerMap = new Map<string, Map<string, TrashMeta[]>>();

  for (const item of items) {
    const provider = item.provider || "claude";
    // Use project_name from trash meta, fallback to path extraction for legacy entries
    let project = item.project_name || labels.unknown;
    if (!item.project_name) {
      const parts = item.original_path.split("/");
      if (parts.length >= 2) {
        project = parts[parts.length - 2];
      }
    }
    if (!providerMap.has(provider)) {
      providerMap.set(provider, new Map());
    }
    const projectMap = providerMap.get(provider)!;
    if (!projectMap.has(project)) {
      projectMap.set(project, []);
    }
    projectMap.get(project)!.push(item);
  }

  const tree: TreeNode[] = [];
  for (const [provider, projectMap] of providerMap) {
    const projectNodes: TreeNode[] = [];
    for (const [project, sessions] of projectMap) {
      const sessionNodes: TreeNode[] = sessions.map((s) => ({
        id: s.id,
        label: s.title || labels.untitled,
        node_type: "session" as const,
        children: [],
        count: 0,
        provider: provider as Provider,
      }));
      projectNodes.push({
        id: `trash-${provider}-${project}`,
        label: project,
        node_type: "project" as const,
        children: sessionNodes,
        count: sessionNodes.length,
        provider: null,
      });
    }
    tree.push({
      id: `trash-${provider}`,
      label: getDisplayLabel(provider as Provider),
      node_type: "provider" as const,
      children: projectNodes,
      count: items.filter((i) => i.provider === provider).length,
      provider: provider as Provider,
    });
  }

  return tree;
}
