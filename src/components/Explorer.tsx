import { createSignal, createEffect, createMemo, For, Show } from "solid-js";
import type { SessionMeta, TreeNode, Provider } from "../lib/types";
import { getResumeCommand, resumeSession, trashSession, exportSessionsBatch, toggleFavorite, renameSession, openInFolder } from "../lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/index";
import { terminalApp, timeGrouping } from "../stores/settings";
import { ContextMenu, type MenuItemDef } from "./ContextMenu";
import { InputDialog } from "./InputDialog";
import { TreeNodeComponent, collectSessionNodes } from "./TreeNode";
import {
  selectedIds,
  toggleSelected,
  clearSelection,
  selectionCount,
} from "../stores/selection";
import { toast, toastError } from "../stores/toast";
import { bumpFavoriteVersion } from "../stores/favorites";

function applyTimeGrouping(tree: TreeNode[], t: (key: string) => string): TreeNode[] {
  const now = Date.now();
  const todayStart = new Date(); todayStart.setHours(0, 0, 0, 0);
  const weekStart = new Date(todayStart); weekStart.setDate(weekStart.getDate() - weekStart.getDay());
  const monthStart = new Date(todayStart); monthStart.setDate(1);

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

function ExplorerSkeleton() {
  return (
    <div class="skeleton-wrapper">
      {Array.from({ length: 3 }).map(() => (
        <div>
          <div class="skeleton-tree-item">
            <div class="skeleton skeleton-tree-dot" />
            <div class="skeleton skeleton-tree-text skeleton-tree-text-sm" />
          </div>
          {Array.from({ length: 4 }).map(() => (
            <div class="skeleton-tree-item skeleton-tree-item-indent">
              <div class="skeleton skeleton-tree-text" />
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

export function Explorer(props: {
  tree: TreeNode[];
  activeSessionId: string | null;
  onOpenSession: (s: SessionMeta) => void;
  onDeleteSession?: (id: string) => void;
  onExportSession?: (id: string) => void;
  onRefreshTree?: () => void;
  isLoading?: boolean;
}) {
  const { t } = useI18n();
  const displayTree = createMemo(() =>
    timeGrouping() ? applyTimeGrouping(props.tree, t) : props.tree
  );
  const [expandedIds, setExpandedIds] = createSignal<Set<string>>(new Set());
  const [initialized, setInitialized] = createSignal(false);

  // Context menu positions — each stores {x,y} or null
  const [sessionMenu, setSessionMenu] = createSignal<{
    pos: { x: number; y: number };
    node: TreeNode;
    projectLabel: string;
  } | null>(null);
  const [nodeMenu, setNodeMenu] = createSignal<{
    pos: { x: number; y: number };
    node: TreeNode;
  } | null>(null);
  const [selectionMenu, setSelectionMenu] = createSignal<{
    x: number;
    y: number;
  } | null>(null);
  const [renameTarget, setRenameTarget] = createSignal<{
    id: string;
    label: string;
  } | null>(null);

  // Auto-expand providers on first load
  createEffect(() => {
    if (props.tree.length > 0 && !initialized()) {
      setExpandedIds(new Set(props.tree.map((n) => n.id)));
      setInitialized(true);
    }
  });

  // Reveal active session: expand parent nodes and scroll into view
  createEffect(() => {
    const sessionId = props.activeSessionId;
    if (!sessionId || props.tree.length === 0) return;
    for (const provider of props.tree) {
      for (const project of provider.children) {
        if (project.children.some((s) => s.id === sessionId)) {
          setExpandedIds((prev) => {
            const next = new Set(prev);
            next.add(provider.id);
            next.add(project.id);
            return next;
          });
          requestAnimationFrame(() => {
            const el = document.querySelector(
              `[data-session-id="${sessionId}"]`
            );
            el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
          });
          return;
        }
      }
    }
  });

  function toggleExpanded(nodeId: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) next.delete(nodeId);
      else next.add(nodeId);
      return next;
    });
  }

  function isNodeExpanded(nodeId: string): boolean {
    return expandedIds().has(nodeId);
  }

  function closeAllMenus() {
    setSessionMenu(null);
    setNodeMenu(null);
    setSelectionMenu(null);
  }

  function buildSessionMeta(
    node: TreeNode,
    parentProjectLabel: string
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

  // --- Click handlers ---

  function handleSessionClick(
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string
  ) {
    if (e.metaKey || e.ctrlKey) {
      toggleSelected(node.id);
      return;
    }
    clearSelection();
    props.onOpenSession(buildSessionMeta(node, parentProjectLabel));
  }

  function handleSessionContextMenu(
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string
  ) {
    setNodeMenu(null);
    setSelectionMenu(null);
    const sel = selectedIds();
    if (sel.size > 1 && sel.has(node.id)) {
      setSessionMenu(null);
      setSelectionMenu({ x: e.clientX, y: e.clientY });
      return;
    }
    setSessionMenu({
      pos: { x: e.clientX, y: e.clientY },
      node,
      projectLabel: parentProjectLabel,
    });
  }

  function handleNodeContextMenu(e: MouseEvent, node: TreeNode) {
    setSessionMenu(null);
    setSelectionMenu(null);
    setNodeMenu({ pos: { x: e.clientX, y: e.clientY }, node });
  }

  // --- Batch operations ---

  async function trashAllUnderNode(node: TreeNode) {
    const sessions = collectSessionNodes(node);
    if (sessions.length === 0) return;
    let failed = 0;
    for (const s of sessions) {
      try {
        await trashSession(
          s.id,
          "",
          (s.provider ?? "claude"),
          s.label
        );
      } catch (e) {
        console.warn("failed to trash session:", s.id, e);
        failed++;
      }
    }
    props.onRefreshTree?.();
    const succeeded = sessions.length - failed;
    if (failed > 0) {
      toastError(`${failed}/${sessions.length} ${t("toast.trashFailed")}`);
    }
    if (succeeded > 0) {
      toast(`${succeeded} ${t("toast.trashed")}`);
    }
  }

  function findSessionInTree(sessionId: string): { provider: string; projectPath: string } | null {
    for (const providerNode of props.tree) {
      for (const projectNode of providerNode.children) {
        for (const sessionNode of projectNode.children) {
          if (sessionNode.id === sessionId) {
            const id = projectNode.id;
            return {
              provider: providerNode.id,
              projectPath: id.includes(":") ? id.slice(id.indexOf(":") + 1) : "",
            };
          }
        }
      }
    }
    return null;
  }

  async function trashSelected() {
    const sel = selectedIds();
    if (sel.size === 0) return;
    let failed = 0;
    for (const id of sel) {
      try {
        const info = findSessionInTree(id);
        await trashSession(id, "", info?.provider ?? "claude", "");
      } catch {
        failed++;
      }
    }
    clearSelection();
    props.onRefreshTree?.();
    if (failed > 0) {
      toastError(`${failed}/${sel.size} ${t("toast.trashFailed")}`);
    } else {
      toast(t("toast.trashed"));
    }
  }

  async function exportSelectedBatch() {
    const sel = selectedIds();
    if (sel.size === 0) return;
    try {
      const outputPath = await save({
        defaultPath: "sessions-export.zip",
        filters: [{ name: "ZIP Archive", extensions: ["zip"] }],
      });
      if (!outputPath) return;

      const items: [string, string, string][] = [];
      for (const id of sel) {
        const info = findSessionInTree(id);
        items.push([id, "", info?.provider ?? "claude"]);
      }

      await exportSessionsBatch(items, "json", outputPath);
      toast(t("toast.copied"));
    } catch (e) {
      toastError(String(e));
    }
  }

  // --- Menu item builders ---

  function sessionMenuItems(): MenuItemDef[] {
    const m = sessionMenu();
    if (!m) return [];
    const { node, projectLabel } = m;
    const sessionProjectPath = findSessionInTree(node.id)?.projectPath ?? "";
    const items: MenuItemDef[] = [
      {
        label: t("contextMenu.openInNewTab"),
        onClick: () => props.onOpenSession(buildSessionMeta(node, projectLabel)),
      },
      {
        label: t("contextMenu.copySessionId"),
        onClick: () => { void navigator.clipboard.writeText(node.id).then(() => toast(t("toast.idCopied"))); },
      },
      {
        label: t("contextMenu.copyResumeCommand"),
        onClick: async () => {
          const provider = (node.provider ?? "claude");
          const cmd = await getResumeCommand(node.id, provider);
          await navigator.clipboard.writeText(cmd);
          toast(t("toast.cmdCopied"));
        },
      },
      ...(sessionProjectPath
        ? [
            {
              label: t("contextMenu.openInFinder"),
              onClick: () => { openInFolder(sessionProjectPath).catch(() => {}); },
            },
            {
              label: t("contextMenu.copyPath"),
              onClick: () => {
                void navigator.clipboard.writeText(sessionProjectPath).then(() => toast(t("toast.copied")));
              },
            },
          ]
        : []),
      { label: "", separator: true, onClick: () => {} },
      {
        label: t("contextMenu.resumeSession"),
        onClick: async () => {
          const provider = (node.provider ?? "claude");
          await resumeSession(node.id, provider, terminalApp());
        },
      },
      { label: "", separator: true, onClick: () => {} },
      {
        label: t("contextMenu.toggleFavorite"),
        onClick: async () => {
          try {
            const newState = await toggleFavorite(node.id);
            bumpFavoriteVersion();
            toast(t(newState ? "toast.favoriteAdded" : "toast.favoriteRemoved"));
          } catch (e) {
            toastError(t("toast.favoriteFailed"));
          }
        },
      },
      {
        label: t("contextMenu.rename"),
        onClick: () => {
          setRenameTarget({ id: node.id, label: node.label });
        },
      },
      { label: "", separator: true, onClick: () => {} },
    ];
    if (props.onExportSession) {
      items.push({
        label: t("contextMenu.export"),
        onClick: () => props.onExportSession?.(node.id),
      });
    }
    if (props.onDeleteSession) {
      items.push({
        label: t("contextMenu.delete"),
        onClick: () => props.onDeleteSession?.(node.id),
      });
    }
    return items;
  }

  function selectionMenuItems(): MenuItemDef[] {
    const items: MenuItemDef[] = [
      {
        label: () =>
          `${t("contextMenu.deleteSelected")} (${selectionCount()})`,
        onClick: trashSelected,
      },
      {
        label: () =>
          `${t("contextMenu.exportSelected")} (${selectionCount()})`,
        onClick: exportSelectedBatch,
      },
    ];
    return items;
  }

  function nodeMenuItems(): MenuItemDef[] {
    const m = nodeMenu();
    if (!m) return [];
    const { node } = m;
    if (node.node_type === "provider") {
      return [
        {
          label: t("contextMenu.collapseAll"),
          onClick: () => collapseAllChildren(node),
        },
        {
          label: t("contextMenu.refresh"),
          onClick: () => props.onRefreshTree?.(),
        },
        { label: "", separator: true, onClick: () => {} },
        {
          label: t("contextMenu.deleteAll"),
          onClick: () => trashAllUnderNode(node),
        },
      ];
    }
    // project — extract path from node.id format "provider:/path/to/project"
    const projectPath = node.id.includes(":") ? node.id.slice(node.id.indexOf(":") + 1) : "";
    const hasPath = projectPath.length > 0;
    return [
      ...(hasPath
        ? [
            {
              label: t("contextMenu.openInFinder"),
              onClick: () => { openInFolder(projectPath).catch(() => {}); },
            },
            {
              label: t("contextMenu.copyPath"),
              onClick: () => {
                void navigator.clipboard.writeText(projectPath).then(() => toast(t("toast.copied")));
              },
            },
            { label: "", separator: true, onClick: () => {} },
          ]
        : []),
      {
        label: t("contextMenu.expandAll"),
        onClick: () => expandAllChildren(node),
      },
      {
        label: t("contextMenu.collapseAll"),
        onClick: () => {
          setExpandedIds((prev) => {
            const next = new Set(prev);
            next.delete(node.id);
            return next;
          });
        },
      },
      { label: "", separator: true, onClick: () => {} },
      {
        label: t("contextMenu.deleteAll"),
        onClick: () => trashAllUnderNode(node),
      },
    ];
  }

  function collapseAllChildren(node: TreeNode) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      for (const child of node.children) {
        next.delete(child.id);
        for (const grandchild of child.children) {
          next.delete(grandchild.id);
        }
      }
      return next;
    });
  }

  function expandAllChildren(node: TreeNode) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      next.add(node.id);
      for (const child of node.children) {
        next.add(child.id);
      }
      return next;
    });
  }

  return (
    <div class="explorer">
      <div class="explorer-header">
        <span>{t("explorer.title")}</span>
        <Show when={selectionCount() > 0}>
          <span class="count-badge-accent">
            {selectionCount()} {t("explorer.selected")}
          </span>
        </Show>
      </div>
      <div class="explorer-tree">
        <Show when={props.isLoading && props.tree.length === 0}>
          <ExplorerSkeleton />
        </Show>
        <For each={displayTree()}>
          {(node) => (
            <TreeNodeComponent
              node={node}
              depth={0}
              activeSessionId={props.activeSessionId}
              isNodeExpanded={isNodeExpanded}
              toggleExpanded={toggleExpanded}
              onSessionContextMenu={handleSessionContextMenu}
              onNodeContextMenu={handleNodeContextMenu}
              onSessionClick={handleSessionClick}
            />
          )}
        </For>
      </div>

      <ContextMenu
        items={sessionMenuItems()}
        position={sessionMenu()?.pos ?? null}
        onClose={closeAllMenus}
      />
      <ContextMenu
        items={selectionMenuItems()}
        position={selectionMenu()}
        onClose={closeAllMenus}
      />
      <ContextMenu
        items={nodeMenuItems()}
        position={nodeMenu()?.pos ?? null}
        onClose={closeAllMenus}
      />

      <InputDialog
        open={renameTarget() !== null}
        title={t("contextMenu.rename")}
        label={t("inputDialog.newTitle")}
        defaultValue={renameTarget()?.label ?? ""}
        confirmLabel={t("inputDialog.rename")}
        onConfirm={async (newTitle) => {
          const target = renameTarget();
          if (target) {
            await renameSession(target.id, newTitle);
            setRenameTarget(null);
            props.onRefreshTree?.();
          }
        }}
        onCancel={() => setRenameTarget(null)}
      />
    </div>
  );
}
