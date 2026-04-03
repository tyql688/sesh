import {
  createSignal,
  createMemo,
  onMount,
  For,
  Show,
  createEffect,
  on,
} from "solid-js";
import { listFavorites } from "../lib/tauri";
import type { SessionMeta, SessionRef, TreeNode } from "../lib/types";
import { useI18n } from "../i18n/index";
import { buildFavoritesTree } from "../lib/tree-builders";
import { toastError } from "../stores/toast";
import { errorMessage } from "../lib/errors";
import { favoriteVersion } from "../stores/favorites";
import { TreeNodeComponent } from "./TreeNode";

export function FavoritesView(props: {
  onOpenSession: (s: SessionRef) => void;
}) {
  const { t } = useI18n();
  const [favorites, setFavorites] = createSignal<SessionMeta[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [expandedIds, setExpandedIds] = createSignal<Set<string>>(new Set());
  const [initialized, setInitialized] = createSignal(false);

  const tree = createMemo(() =>
    buildFavoritesTree(favorites(), t("explorer.noProject")),
  );

  function autoExpand(nodes: TreeNode[]) {
    const ids = new Set<string>();
    for (const n of nodes) {
      ids.add(n.id);
      for (const c of n.children) {
        ids.add(c.id);
      }
    }
    return ids;
  }

  async function refresh() {
    try {
      const data = await listFavorites();
      setFavorites(data);
      if (!initialized()) {
        setExpandedIds(
          autoExpand(buildFavoritesTree(data, t("explorer.noProject"))),
        );
        setInitialized(true);
      }
    } catch (e) {
      toastError(errorMessage(e));
    } finally {
      setLoading(false);
    }
  }

  onMount(refresh);

  // Re-fetch when favorite version changes (e.g. toggled from Explorer or SessionView)
  createEffect(
    on(favoriteVersion, () => {
      if (initialized()) refresh();
    }),
  );

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

  function findSession(id: string): SessionMeta | undefined {
    return favorites().find((s) => s.id === id);
  }

  function handleSessionClick(_e: MouseEvent, node: TreeNode) {
    const session = findSession(node.id);
    if (session) {
      props.onOpenSession(session);
    }
  }

  // no-op for context menus — no special behavior
  function handleContextMenu(_e: MouseEvent, _node: TreeNode) {}

  return (
    <div class="favorites-view">
      <div class="explorer-header">
        <span>{t("favorites.title")}</span>
        <Show when={favorites().length > 0}>
          <span class="count-badge">{favorites().length}</span>
        </Show>
      </div>
      <Show when={loading()}>
        <div class="loading-center">
          <div class="spinner spinner-sm" />
        </div>
      </Show>
      <Show when={!loading() && favorites().length === 0}>
        <div class="empty-state">
          <svg
            width="32"
            height="32"
            fill="none"
            stroke="var(--text-tertiary)"
            stroke-width="1.5"
            viewBox="0 0 24 24"
          >
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
          <p class="empty-state-text">{t("favorites.empty")}</p>
          <p class="empty-state-hint">{t("favorites.emptyHint")}</p>
        </div>
      </Show>
      <Show when={!loading() && favorites().length > 0}>
        <div class="explorer-tree">
          <For each={tree()}>
            {(node) => (
              <TreeNodeComponent
                node={node}
                depth={0}
                activeSessionId={null}
                isNodeExpanded={isNodeExpanded}
                toggleExpanded={toggleExpanded}
                onSessionContextMenu={(e, n, _p) => handleContextMenu(e, n)}
                onNodeContextMenu={handleContextMenu}
                onSessionClick={(e, n, _p) => handleSessionClick(e, n)}
              />
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
