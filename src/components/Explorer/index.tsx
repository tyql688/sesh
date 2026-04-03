import { createSignal, createEffect, createMemo, For, Show } from "solid-js";
import type { SessionRef, TreeNode } from "../../lib/types";
import {
  resumeSession,
  trashSession,
  exportSessionsBatch,
  toggleFavorite,
  renameSession,
} from "../../lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../../i18n/index";
import {
  terminalApp,
  timeGrouping,
  showOrphans,
  addBlockedFolder,
} from "../../stores/settings";
import { ContextMenu } from "../ContextMenu";
import { InputDialog } from "../InputDialog";
import { TreeNodeComponent, collectSessionNodes } from "../TreeNode";
import {
  selectedIds,
  toggleSelected,
  clearSelection,
  selectionCount,
} from "../../stores/selection";
import { toast, toastError } from "../../stores/toast";
import { errorMessage } from "../../lib/errors";
import {
  filterBlockedFolders,
  applyTimeGrouping,
  filterOrphanSubagents,
  buildSessionRef,
} from "./hooks";
import {
  buildSessionMenuItems,
  buildSelectionMenuItems,
  buildNodeMenuItems,
} from "./ContextMenus";

function ExplorerSkeleton() {
  return (
    <div class="skeleton-wrapper">
      {/* eslint-disable-next-line solid/prefer-for */}
      {Array.from({ length: 3 }).map(() => (
        <div>
          <div class="skeleton-tree-item">
            <div class="skeleton skeleton-tree-dot" />
            <div class="skeleton skeleton-tree-text skeleton-tree-text-sm" />
          </div>
          {/* eslint-disable-next-line solid/prefer-for */}
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
  onOpenSession: (s: SessionRef) => void;
  onDeleteSession?: (id: string) => void;
  onExportSession?: (id: string) => void;
  onRefreshTree?: () => void;
  isLoading?: boolean;
}) {
  const { t } = useI18n();
  const displayTree = createMemo(() => {
    let tree = filterBlockedFolders(props.tree);
    if (!showOrphans()) tree = filterOrphanSubagents(tree);
    return timeGrouping() ? applyTimeGrouping(tree, t) : tree;
  });
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

  // Reveal active session: expand ancestor nodes and scroll into view.
  // Must search displayTree (not props.tree) because time grouping inserts
  // intermediate nodes with different IDs.
  createEffect(() => {
    const sessionId = props.activeSessionId;
    const tree = displayTree();
    if (!sessionId || tree.length === 0) return;

    // DFS: find the path of ancestor node IDs leading to the target session
    function findPath(nodes: TreeNode[], target: string): string[] | null {
      for (const node of nodes) {
        if (node.id === target) return [];
        const sub = findPath(node.children, target);
        if (sub !== null) return [node.id, ...sub];
      }
      return null;
    }

    const path = findPath(tree, sessionId);
    if (!path) return;

    setExpandedIds((prev) => {
      const next = new Set(prev);
      for (const id of path) next.add(id);
      return next;
    });
    requestAnimationFrame(() => {
      const el = document.querySelector(`[data-session-id="${sessionId}"]`);
      el?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    });
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

  // --- Click handlers ---

  function handleSessionClick(
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string,
  ) {
    if (e.metaKey || e.ctrlKey) {
      toggleSelected(node.id);
      return;
    }
    clearSelection();
    props.onOpenSession(buildSessionRef(node, parentProjectLabel));
  }

  function handleSessionContextMenu(
    e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string,
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
    // If there are selected sessions, show selection menu instead of node menu
    if (selectionCount() > 0) {
      setNodeMenu(null);
      setSelectionMenu({ x: e.clientX, y: e.clientY });
      return;
    }
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
        await trashSession(s.id, "", s.provider ?? "claude", s.label);
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

  function findSessionInTree(sessionId: string): {
    provider: string;
    projectPath: string;
    providerLabel: string;
  } | null {
    function search(
      nodes: TreeNode[],
      providerHint: string,
      providerLabelHint: string,
      displayKeyHint: string,
      projectHint: string,
    ): {
      provider: string;
      projectPath: string;
      providerLabel: string;
    } | null {
      for (const node of nodes) {
        if (node.node_type === "session" && node.id === sessionId) {
          return {
            provider: providerHint,
            projectPath: projectHint,
            providerLabel: providerLabelHint,
          };
        }
        if (node.children && node.children.length > 0) {
          const nextProvider =
            node.node_type === "provider"
              ? (node.provider ?? node.id)
              : providerHint;
          const nextProviderLabel =
            node.node_type === "provider" ? node.label : providerLabelHint;
          // displayKey is the provider tree node id (e.g. "claude", "cc-mirror:cczai").
          // Project node id is "displayKey:/path". Strip displayKey + ":" to get path.
          const nextDisplayKey =
            node.node_type === "provider" ? node.id : displayKeyHint;
          const nextProject =
            node.node_type === "project" && !providerHint
              ? ""
              : node.node_type === "project" && providerHint && !projectHint
                ? (node.project_path ?? "")
                : projectHint;
          const result = search(
            node.children,
            nextProvider,
            nextProviderLabel,
            nextDisplayKey,
            nextProject,
          );
          if (result) return result;
        }
      }
      return null;
    }
    return search(props.tree, "", "", "", "");
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
      toastError(errorMessage(e));
    }
  }

  // --- Menu item builders ---

  function sessionMenuItems() {
    const m = sessionMenu();
    if (!m) return [];
    const sessionInfo = findSessionInTree(m.node.id);
    return buildSessionMenuItems({
      node: m.node,
      sessionProjectPath: sessionInfo?.projectPath ?? "",
      providerLabel: sessionInfo?.providerLabel,
      t,
      terminalApp: terminalApp(),
      resumeSession,
      toggleFavorite,
      setRenameTarget,
      onExportSession: props.onExportSession,
      onDeleteSession: props.onDeleteSession,
    });
  }

  function selectionMenuItems() {
    return buildSelectionMenuItems({
      t,
      trashSelected,
      exportSelectedBatch,
    });
  }

  function nodeMenuItems() {
    const m = nodeMenu();
    if (!m) return [];
    return buildNodeMenuItems({
      node: m.node,
      t,
      collapseAllChildren,
      expandAllChildren,
      collapseNode: (nodeId: string) => {
        setExpandedIds((prev) => {
          const next = new Set(prev);
          next.delete(nodeId);
          return next;
        });
      },
      trashAllUnderNode,
      onRefreshTree: props.onRefreshTree,
      addBlockedFolder,
    });
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
