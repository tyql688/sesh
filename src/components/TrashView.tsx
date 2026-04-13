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
  restoreSessionsBatch,
  emptyTrash,
  permanentDeleteTrash,
  permanentDeleteTrashBatch,
} from "../lib/tauri";
import { ProviderDot } from "../lib/icons";
import { collectSessionIds } from "../lib/tree-utils";
import { buildTrashTree } from "../lib/tree-builders";
import { useI18n } from "../i18n/index";
import { toast, toastError } from "../stores/toast";
import { errorMessage } from "../lib/errors";
import { formatAbsoluteTime } from "../lib/formatters";
import { ConfirmDialog } from "./ConfirmDialog";

export function TrashView(props: { onRefreshTree: () => void }) {
  const { t } = useI18n();
  const [showEmptyConfirm, setShowEmptyConfirm] = createSignal(false);
  const [showRestoreConfirm, setShowRestoreConfirm] = createSignal(false);
  const [restoreTarget, setRestoreTarget] = createSignal<TreeNode | null>(null);
  const [showDeleteAllConfirm, setShowDeleteAllConfirm] = createSignal(false);
  const [deleteAllTarget, setDeleteAllTarget] = createSignal<TreeNode | null>(
    null,
  );
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
    const ids = new Set<string>();
    const collectIds = (nodes: TreeNode[]) => {
      for (const node of nodes) {
        if (node.node_type !== "session") {
          ids.add(node.id);
          collectIds(node.children);
        }
      }
    };
    collectIds(trashTree);
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
      toastError(`${t("trash.restore")}: ${errorMessage(e)}`);
    }
  }

  async function handlePermanentDelete(id: string) {
    try {
      await permanentDeleteTrash(id);
      refetch();
    } catch (e) {
      toastError(errorMessage(e));
    }
  }

  async function handleEmptyTrash() {
    try {
      await emptyTrash();
      setShowEmptyConfirm(false);
      refetch();
    } catch (e) {
      toastError(errorMessage(e));
      setShowEmptyConfirm(false);
    }
  }

  async function handleRestoreAll(node: TreeNode) {
    const ids = collectSessionIds(node);
    const result = await restoreSessionsBatch(ids);
    await Promise.all([refetch(), props.onRefreshTree()]);
    if (result.failed > 0)
      toastError(
        `${result.failed}/${result.succeeded + result.failed} ${t("trash.restore")}`,
      );
    else toast(`${result.succeeded} ${t("trash.restoreOk")}`);
  }

  async function handleDeleteAll(node: TreeNode) {
    const ids = collectSessionIds(node);
    await permanentDeleteTrashBatch(ids);
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
    const isGroup = () => nodeProps.node.node_type === "project";
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
          <Show when={isGroup()}>
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
                  setRestoreTarget(nodeProps.node);
                  setShowRestoreConfirm(true);
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
                  setDeleteAllTarget(nodeProps.node);
                  setShowDeleteAllConfirm(true);
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
              {formatAbsoluteTime(trashItem()!.trashed_at)}
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

      <ConfirmDialog
        open={showDeleteAllConfirm()}
        title={t("trash.permanentDelete")}
        message={t("trash.deleteAllConfirm")}
        confirmLabel={t("trash.permanentDelete")}
        onConfirm={async () => {
          const node = deleteAllTarget();
          setShowDeleteAllConfirm(false);
          setDeleteAllTarget(null);
          if (node) await handleDeleteAll(node);
        }}
        onCancel={() => {
          setShowDeleteAllConfirm(false);
          setDeleteAllTarget(null);
        }}
        danger={true}
      />

      <ConfirmDialog
        open={showRestoreConfirm()}
        title={t("trash.restore")}
        message={t("trash.restoreAllConfirm")}
        confirmLabel={t("trash.restore")}
        onConfirm={async () => {
          const node = restoreTarget();
          setShowRestoreConfirm(false);
          setRestoreTarget(null);
          if (node) await handleRestoreAll(node);
        }}
        onCancel={() => {
          setShowRestoreConfirm(false);
          setRestoreTarget(null);
        }}
      />
    </div>
  );
}
