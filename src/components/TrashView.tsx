import {
  createSignal,
  createMemo,
  createResource,
  For,
  Show,
  onMount,
} from "solid-js";
import type { TrashMeta } from "../lib/types";
import type { TreeNode } from "../lib/types";
import {
  listTrash,
  restoreSession,
  emptyTrash,
  permanentDeleteTrash,
} from "../lib/tauri";
import { ProviderDot } from "../lib/icons";
import { collectSessionIds } from "../lib/tree-utils";
import { buildTrashTree } from "../lib/tree-builders";
import { useI18n } from "../i18n/index";
import { toast, toastError } from "../stores/toast";
import { ConfirmDialog } from "./ConfirmDialog";

function formatTrashDate(epoch: number): string {
  if (!epoch) return "-";
  const d = new Date(epoch * 1000);
  return d.toLocaleString();
}

export function TrashView(props: { onRefreshTree: () => void }) {
  const { t } = useI18n();
  const [showEmptyConfirm, setShowEmptyConfirm] = createSignal(false);
  const [expandedIds, setExpandedIds] = createSignal<Set<string>>(new Set());

  const [trashItems, { refetch }] = createResource<TrashMeta[]>(async () => {
    try {
      return await listTrash();
    } catch {
      return [];
    }
  });

  onMount(() => refetch());

  const tree = createMemo(() => {
    const items = trashItems() || [];
    const trashTree = buildTrashTree(items, {
      unknown: t("common.unknown"),
      untitled: t("common.untitled"),
    });
    // Auto-expand all
    const ids = new Set<string>();
    for (const n of trashTree) {
      ids.add(n.id);
      for (const c of n.children) ids.add(c.id);
    }
    setExpandedIds(ids);
    return trashTree;
  });

  const itemMap = createMemo(() => {
    const map = new Map<string, TrashMeta>();
    for (const item of trashItems() || []) {
      map.set(item.id, item);
    }
    return map;
  });

  async function handleRestore(id: string) {
    try {
      await restoreSession(id);
      await Promise.all([refetch(), props.onRefreshTree()]);
      toast(t("trash.restoreOk"));
    } catch (e) {
      await refetch();
      toastError(`${t("trash.restore")}: ${String(e)}`);
    }
  }

  async function handlePermanentDelete(id: string) {
    try {
      await permanentDeleteTrash(id);
      refetch();
    } catch (e) {
      toastError(String(e));
    }
  }

  async function handleEmptyTrash() {
    try {
      await emptyTrash();
      setShowEmptyConfirm(false);
      refetch();
    } catch (e) {
      toastError(String(e));
      setShowEmptyConfirm(false);
    }
  }

  async function handleRestoreAll(node: TreeNode) {
    const ids = collectSessionIds(node);
    let failed = 0;
    for (const id of ids) {
      try {
        await restoreSession(id);
      } catch {
        failed++;
      }
    }
    await Promise.all([refetch(), props.onRefreshTree()]);
    if (failed > 0) toastError(`${failed}/${ids.length} ${t("trash.restore")}`);
    else toast(`${ids.length} ${t("trash.restoreOk")}`);
  }

  async function handleDeleteAll(node: TreeNode) {
    const ids = collectSessionIds(node);
    for (const id of ids) {
      try {
        await permanentDeleteTrash(id);
      } catch {
        /* skip */
      }
    }
    refetch();
  }

  function toggleExpanded(nodeId: string) {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) next.delete(nodeId);
      else next.add(nodeId);
      return next;
    });
  }

  function TrashTreeNode(nodeProps: { node: TreeNode; depth: number }) {
    const isLeaf = () => nodeProps.node.node_type === "session";
    const isProject = () => nodeProps.node.node_type === "project";
    const expanded = () => expandedIds().has(nodeProps.node.id);
    const trashItem = () => itemMap().get(nodeProps.node.id);

    return (
      <div>
        <div
          class={`trash-tree-node${isLeaf() ? " trash-tree-leaf" : " trash-tree-group"}`}
          style={{ "padding-left": `${nodeProps.depth * 16 + 12}px` }}
          onClick={() => !isLeaf() && toggleExpanded(nodeProps.node.id)}
        >
          <Show when={!isLeaf()}>
            <svg
              width="14"
              height="14"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              viewBox="0 0 24 24"
              class={`chevron${expanded() ? " expanded" : ""}`}
            >
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </Show>
          <Show when={isLeaf()}>
            <span class="trash-tree-spacer" />
          </Show>

          <Show
            when={
              nodeProps.node.node_type === "provider" && nodeProps.node.provider
            }
          >
            <ProviderDot provider={nodeProps.node.provider!} />
          </Show>
          <Show when={isProject()}>
            <span class="trash-tree-icon">
              <svg
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                viewBox="0 0 24 24"
              >
                <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z" />
              </svg>
            </span>
          </Show>
          <Show when={isLeaf()}>
            <span class="trash-tree-icon trash-tree-icon-session">
              <svg
                width="14"
                height="14"
                fill="none"
                stroke="currentColor"
                stroke-width="1.5"
                viewBox="0 0 24 24"
              >
                <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
              </svg>
            </span>
          </Show>

          <span
            class={`trash-tree-label${nodeProps.node.node_type === "provider" ? " bold" : ""}`}
            title={isLeaf() ? nodeProps.node.label : undefined}
          >
            {isLeaf()
              ? nodeProps.node.label.length > 50
                ? nodeProps.node.label.slice(0, 47) + "..."
                : nodeProps.node.label
              : nodeProps.node.label}
          </span>

          <Show when={!isLeaf() && nodeProps.node.count > 0}>
            <span class="tree-node-count">{nodeProps.node.count}</span>
            <div class="trash-tree-actions">
              <button
                class="trash-action-btn trash-action-btn-restore"
                onClick={(e) => {
                  e.stopPropagation();
                  handleRestoreAll(nodeProps.node);
                }}
                title={t("trash.restore")}
              >
                <svg
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  viewBox="0 0 24 24"
                >
                  <polyline points="1 4 1 10 7 10" />
                  <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
                </svg>
              </button>
              <button
                class="trash-action-btn trash-action-btn-danger"
                onClick={(e) => {
                  e.stopPropagation();
                  handleDeleteAll(nodeProps.node);
                }}
                title={t("trash.permanentDelete")}
              >
                <svg
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  viewBox="0 0 24 24"
                >
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          </Show>

          <Show when={isLeaf() && trashItem()}>
            <span class="trash-tree-date">
              {formatTrashDate(trashItem()!.trashed_at)}
            </span>
            <div class="trash-tree-actions">
              <button
                class="trash-action-btn trash-action-btn-restore"
                onClick={(e) => {
                  e.stopPropagation();
                  handleRestore(nodeProps.node.id);
                }}
                title={t("trash.restore")}
              >
                <svg
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  viewBox="0 0 24 24"
                >
                  <polyline points="1 4 1 10 7 10" />
                  <path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
                </svg>
              </button>
              <button
                class="trash-action-btn trash-action-btn-danger"
                onClick={(e) => {
                  e.stopPropagation();
                  handlePermanentDelete(nodeProps.node.id);
                }}
                title={t("trash.permanentDelete")}
              >
                <svg
                  width="12"
                  height="12"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  viewBox="0 0 24 24"
                >
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              </button>
            </div>
          </Show>
        </div>

        <Show when={expanded() && !isLeaf()}>
          <For each={nodeProps.node.children}>
            {(child) => (
              <TrashTreeNode node={child} depth={nodeProps.depth + 1} />
            )}
          </For>
        </Show>
      </div>
    );
  }

  return (
    <div class="trash-view">
      <div class="trash-header">
        <span class="trash-title">
          {t("trash.title")}
          <Show when={trashItems() && trashItems()!.length > 0}>
            <span class="trash-count"> ({trashItems()!.length})</span>
          </Show>
        </span>
        <Show when={trashItems() && trashItems()!.length > 0}>
          <button
            class="trash-empty-btn"
            onClick={() => setShowEmptyConfirm(true)}
          >
            {t("trash.emptyTrash")}
          </button>
        </Show>
      </div>

      <div class="trash-list">
        <Show
          when={
            !trashItems.loading && trashItems() && trashItems()!.length === 0
          }
        >
          <div class="trash-empty-state">
            <svg
              class="icon-faded"
              width="32"
              height="32"
              fill="none"
              stroke="currentColor"
              stroke-width="1"
              viewBox="0 0 24 24"
            >
              <polyline points="3 6 5 6 21 6" />
              <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
            </svg>
            <span>{t("trash.empty")}</span>
          </div>
        </Show>

        <Show when={trashItems.loading}>
          <div class="trash-empty-state">
            <div class="spinner spinner-sm" />
          </div>
        </Show>

        <For each={tree()}>
          {(node) => <TrashTreeNode node={node} depth={0} />}
        </For>
      </div>

      <ConfirmDialog
        open={showEmptyConfirm()}
        title={t("trash.emptyTrash")}
        message={t("trash.emptyTrashConfirm")}
        confirmLabel={t("trash.emptyTrash")}
        onConfirm={handleEmptyTrash}
        onCancel={() => setShowEmptyConfirm(false)}
        danger={true}
      />
    </div>
  );
}
