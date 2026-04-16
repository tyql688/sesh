import { createSignal, createEffect, createMemo, For, Show } from "solid-js";
import type { SessionRef, TreeNode } from "../../lib/types";
import {
  getResumeCommand,
  resumeSession,
  trashSessionsBatch,
  exportSessionsBatch,
  toggleFavorite,
  renameSession,
  invokeWithFallback,
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
  onPreviewSession: (s: SessionRef) => void;
  onDeleteSession?: (id: string) => void;
  onExportSession?: (id: string) => void;
  onRefreshTree?: () => void;
  onCollapse?: () => void;
  isLoading?: boolean;
}) {
  const { t } = useI18n();
  const displayTree = createMemo(() => {
    let tree = filterBlockedFolders(props.tree);
    if (!showOrphans()) tree = filterOrphanSubagents(tree);
    return timeGrouping() ? applyTimeGrouping(tree, t) : tree;
  });

  // O(1) session ID → project path lookup, rebuilt when props.tree changes
  const sessionProjectPathMap = createMemo(() => {
    const map = new Map<string, string>();
    function walk(
      nodes: TreeNode[],
      providerHint: string,
      projectHint: string,
    ) {
      for (const node of nodes) {
        if (node.node_type === "session") {
          map.set(node.id, projectHint);
        }
        if (node.children && node.children.length > 0) {
          const nextProvider =
            node.node_type === "provider"
              ? (node.provider ?? node.id)
              : providerHint;
          const nextProject =
            node.node_type === "project" && !providerHint
              ? ""
              : node.node_type === "project" && providerHint && !projectHint
                ? (node.project_path ?? "")
                : projectHint;
          walk(node.children, nextProvider, nextProject);
        }
      }
    }
    walk(props.tree, "", "");
    return map;
  });
  const [expandedIds, setExpandedIds] = createSignal<Set<string>>(new Set());
  const [initialized, setInitialized] = createSignal(false);

  // Context menu positions — each stores {x,y} or null
  const [sessionMenu, setSessionMenu] = createSignal<{
    pos: { x: number; y: number };
    node: TreeNode;
    projectLabel: string;
    resumeCommand: string | null;
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

  // Reveal active session on demand: expand ancestors and scroll into view.
  function revealActiveSession() {
    const sessionId = props.activeSessionId;
    const tree = displayTree();
    if (!sessionId || tree.length === 0) return;

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
  }

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
    props.onPreviewSession(buildSessionRef(node, parentProjectLabel));
  }

  function handleSessionDblClick(
    _e: MouseEvent,
    node: TreeNode,
    parentProjectLabel: string,
  ) {
    props.onOpenSession(buildSessionRef(node, parentProjectLabel));
  }

  const resumeCommandCache = new Map<string, string | null>();

  async function handleSessionContextMenu(
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
    let resumeCommand = resumeCommandCache.get(node.id) ?? null;
    if (!resumeCommandCache.has(node.id)) {
      resumeCommand = await invokeWithFallback(
        getResumeCommand(node.id),
        null,
        `load resume command for session ${node.id}`,
      );
      resumeCommandCache.set(node.id, resumeCommand);
    }
    setSessionMenu({
      pos: { x: e.clientX, y: e.clientY },
      node,
      projectLabel: parentProjectLabel,
      resumeCommand,
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
    const result = await trashSessionsBatch(sessions.map((s) => s.id));
    props.onRefreshTree?.();
    if (result.failed > 0) {
      toastError(
        `${result.failed}/${result.succeeded + result.failed} ${t("toast.trashFailed")}`,
      );
    }
    if (result.succeeded > 0) {
      toast(`${result.succeeded} ${t("toast.trashed")}`);
    }
  }

  function findSessionProjectPath(sessionId: string): string {
    return sessionProjectPathMap().get(sessionId) ?? "";
  }

  async function trashSelected() {
    const sel = selectedIds();
    if (sel.size === 0) return;
    const result = await trashSessionsBatch([...sel]);
    clearSelection();
    props.onRefreshTree?.();
    if (result.failed > 0) {
      toastError(
        `${result.failed}/${result.succeeded + result.failed} ${t("toast.trashFailed")}`,
      );
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

      await exportSessionsBatch([...sel], "json", outputPath);
      toast(t("toast.copied"));
    } catch (e) {
      toastError(errorMessage(e));
    }
  }

  // --- Menu item builders ---

  function sessionMenuItems() {
    const m = sessionMenu();
    if (!m) return [];
    return buildSessionMenuItems({
      node: m.node,
      sessionProjectPath: findSessionProjectPath(m.node.id),
      resumeCommand: m.resumeCommand,
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
      function removeAll(n: TreeNode) {
        for (const child of n.children) {
          next.delete(child.id);
          removeAll(child);
        }
      }
      removeAll(node);
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

  // Drag-to-resize handle
  let explorerRef: HTMLDivElement | undefined;

  function onResizeStart(e: MouseEvent) {
    e.preventDefault();
    const el = explorerRef;
    if (!el) return;
    const startX = e.clientX;
    const startW = el.offsetWidth;
    const handle = e.currentTarget as HTMLElement;
    handle.classList.add("active");

    function onMove(ev: MouseEvent) {
      const w = Math.max(
        160,
        Math.min(startW + ev.clientX - startX, window.innerWidth * 0.5),
      );
      el!.style.width = `${w}px`;
    }
    function onUp() {
      handle.classList.remove("active");
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    }
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }

  return (
    <div class="explorer" ref={explorerRef}>
      <div class="explorer-resize-handle" onMouseDown={onResizeStart} />
      <div class="explorer-header">
        <span>{t("explorer.title")}</span>
        <Show when={selectionCount() > 0}>
          <span class="count-badge-accent">
            {selectionCount()} {t("explorer.selected")}
          </span>
        </Show>
        <span class="explorer-header-actions">
          <Show when={props.activeSessionId}>
            <button
              class="explorer-header-btn"
              title={t("explorer.locateSession")}
              onClick={revealActiveSession}
            >
              {"\u2316"}
            </button>
          </Show>
          <Show when={props.onCollapse}>
            <button
              class="explorer-header-btn"
              title={t("explorer.hideExplorer")}
              onClick={() => props.onCollapse?.()}
            >
              {"\u2190"}
            </button>
          </Show>
        </span>
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
              onSessionDblClick={handleSessionDblClick}
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
        maxLength={200}
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
