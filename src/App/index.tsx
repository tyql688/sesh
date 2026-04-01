import {
  createSignal,
  createMemo,
  onMount,
  onCleanup,
  Show,
  ErrorBoundary,
} from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ActivityBar } from "../components/ActivityBar";
import { Explorer } from "../components/Explorer";
import { EditorArea } from "../components/EditorArea";
import { StatusBar } from "../components/StatusBar";
import { SearchPanel } from "../components/SearchPanel";
import { SettingsPanel } from "../components/SettingsPanel";
import { TrashView } from "../components/TrashView";

import { FavoritesView } from "../components/FavoritesView";
import { BlockedView } from "../components/BlockedView";
import { KeyboardOverlay } from "../components/KeyboardOverlay";
import { ToastContainer } from "../components/ToastContainer";
import { trashSession, getChildSessions } from "../lib/tauri";
import { isMac, isWindows } from "../lib/platform";
import { disabledProviders } from "../stores/settings";
import { toastError } from "../stores/toast";
import type { TreeNode, SessionMeta, Provider } from "../lib/types";
import { useI18n } from "../i18n";
import { createKeyboardHandler } from "./KeyboardShortcuts";
import { createSyncManager } from "./SyncManager";
import "../styles/index.css";

export default function App() {
  const { t } = useI18n();
  const [tree, setTree] = createSignal<TreeNode[]>([]);
  const [sessionCount, setSessionCount] = createSignal(0);
  const [activeView, setActiveView] = createSignal("explorer");
  const [openTabs, setOpenTabs] = createSignal<SessionMeta[]>([]);
  const [activeTabId, setActiveTabId] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(true);
  const [showKeyboardOverlay, setShowKeyboardOverlay] = createSignal(false);

  const debouncedChangedPaths = new Set<string>();

  let unlistenWatcher: UnlistenFn | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function syncTabsWithTree(treeData: TreeNode[]) {
    const titleMap = new Map<string, string>();
    function walk(node: TreeNode) {
      if (node.node_type === "session") {
        titleMap.set(node.id, node.label);
      }
      for (const child of node.children) walk(child);
    }
    for (const n of treeData) walk(n);

    setOpenTabs((prev) =>
      prev.map((tab) => {
        const newTitle = titleMap.get(tab.id);
        if (newTitle && newTitle !== tab.title) {
          return { ...tab, title: newTitle };
        }
        return tab;
      }),
    );
  }

  const sync = createSyncManager({
    setTree,
    setSessionCount,
    setIsLoading,
    syncTabsWithTree,
  });

  function openSession(session: SessionMeta) {
    const tabs = openTabs();
    if (!tabs.find((t) => t.id === session.id)) {
      setOpenTabs([...tabs, session]);
    }
    setActiveTabId(session.id);
  }

  function closeTab(sessionId: string) {
    const tabs = openTabs().filter((t) => t.id !== sessionId);
    setOpenTabs(tabs);
    if (activeTabId() === sessionId) {
      setActiveTabId(tabs.length > 0 ? tabs[tabs.length - 1].id : null);
    }
  }

  function closeAllTabs() {
    setOpenTabs([]);
    setActiveTabId(null);
  }

  function closeOtherTabs(keepId: string) {
    const kept = openTabs().filter((t) => t.id === keepId);
    setOpenTabs(kept);
    setActiveTabId(keepId);
  }

  function closeTabsToRight(fromId: string) {
    const tabs = openTabs();
    const idx = tabs.findIndex((t) => t.id === fromId);
    if (idx === -1) return;
    const kept = tabs.slice(0, idx + 1);
    setOpenTabs(kept);
    const currentActive = activeTabId();
    if (currentActive && !kept.find((t) => t.id === currentActive)) {
      setActiveTabId(fromId);
    }
  }

  const handleGlobalKeyDown = createKeyboardHandler({
    activeTabId,
    openTabs,
    showKeyboardOverlay,
    setActiveTabId,
    setShowKeyboardOverlay,
    setActiveView,
    closeTab,
    closeAllTabs,
    syncFromDisk: sync.syncFromDisk,
  });

  onMount(async () => {
    if (isMac) {
      document.documentElement.style.setProperty("--titlebar-inset", "78px");
    }

    void sync.coldStart();

    document.addEventListener("keydown", handleGlobalKeyDown);

    // Listen for subagent open requests from ToolMessage
    const handleOpenSubagent = async (e: Event) => {
      const { description, nickname } = (e as CustomEvent).detail;
      const activeTab = openTabs().find((t) => t.id === activeTabId());
      if (!activeTab) return;
      try {
        const children = await getChildSessions(activeTab.id);
        const match = children.find(
          (c) =>
            (nickname && c.title === nickname) ||
            (description && c.title === description),
        );
        if (match) {
          openSession(match);
        }
      } catch {
        // silently ignore if query fails
      }
    };
    window.addEventListener("open-subagent", handleOpenSubagent);

    unlistenWatcher = await listen<string[]>("sessions-changed", (event) => {
      for (const path of event.payload ?? []) {
        if (path.length > 0) {
          debouncedChangedPaths.add(path);
        }
      }
      clearTimeout(debounceTimer);
      debounceTimer = setTimeout(() => {
        const changedPaths = [...debouncedChangedPaths];
        debouncedChangedPaths.clear();
        void sync.syncFromDisk({ changedPaths });
      }, 500);
    });
  });

  onCleanup(() => {
    document.removeEventListener("keydown", handleGlobalKeyDown);
    unlistenWatcher?.();
    clearTimeout(debounceTimer);
    debouncedChangedPaths.clear();
  });

  const filteredTree = createMemo(() =>
    tree().filter((node) => !disabledProviders().includes(node.id as Provider)),
  );
  const showExplorer = createMemo(() => {
    const v = activeView();
    return v !== "settings" && v !== "trash";
  });
  const showExplorerTree = createMemo(() => {
    const v = activeView();
    return (
      v !== "settings" && v !== "trash" && v !== "favorites" && v !== "blocked"
    );
  });

  return (
    <ErrorBoundary
      fallback={(err) => (
        <div
          style={{
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            "justify-content": "center",
            height: "100vh",
            gap: "16px",
            padding: "24px",
            "text-align": "center",
            "font-family": "var(--font-family)",
            color: "var(--text-primary)",
            background: "var(--bg-primary)",
          }}
        >
          <h2>{t("error.title")}</h2>
          <p style={{ color: "var(--text-secondary)", "max-width": "500px" }}>
            {err?.message || t("error.message")}
          </p>
          <button
            onClick={() => window.location.reload()}
            style={{
              padding: "8px 16px",
              "border-radius": "6px",
              border: "1px solid var(--border-color)",
              background: "var(--bg-secondary)",
              color: "var(--text-primary)",
              cursor: "pointer",
            }}
          >
            {t("error.reload")}
          </button>
        </div>
      )}
    >
      <div class="app-layout">
        <div
          class="titlebar"
          onMouseDown={(e) => {
            if (e.buttons !== 1) return;
            if (
              (e.target as HTMLElement).closest("input, button, .search-panel")
            )
              return;
            e.preventDefault();
            if (e.detail === 2) {
              getCurrentWindow().toggleMaximize();
            } else {
              getCurrentWindow().startDragging();
            }
          }}
        >
          <div class="titlebar-center">
            <span class="app-name">
              <span class="app-name-bracket">&lt;</span>cc-session
              <span class="app-name-bracket">/&gt;</span>
            </span>
          </div>
          <div class="titlebar-right">
            <SearchPanel onOpenSession={openSession} />
          </div>
          <Show when={isWindows}>
            <div class="win-controls">
              <button
                class="win-ctrl-btn"
                onClick={() => getCurrentWindow().minimize()}
              >
                <svg viewBox="0 0 10 1">
                  <rect width="10" height="1" />
                </svg>
              </button>
              <button
                class="win-ctrl-btn"
                onClick={() => getCurrentWindow().toggleMaximize()}
              >
                <svg viewBox="0 0 10 10">
                  <rect
                    x="0.5"
                    y="0.5"
                    width="9"
                    height="9"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1"
                  />
                </svg>
              </button>
              <button
                class="win-ctrl-btn close"
                onClick={() => getCurrentWindow().close()}
              >
                <svg viewBox="0 0 10 10">
                  <line
                    x1="0"
                    y1="0"
                    x2="10"
                    y2="10"
                    stroke="currentColor"
                    stroke-width="1.2"
                  />
                  <line
                    x1="10"
                    y1="0"
                    x2="0"
                    y2="10"
                    stroke="currentColor"
                    stroke-width="1.2"
                  />
                </svg>
              </button>
            </div>
          </Show>
        </div>
        <div class="main-layout">
          <ActivityBar activeView={activeView()} onViewChange={setActiveView} />
          <Show when={showExplorerTree()}>
            <Explorer
              tree={filteredTree()}
              isLoading={isLoading()}
              activeSessionId={activeTabId()}
              onOpenSession={openSession}
              onRefreshTree={sync.refreshTree}
              onDeleteSession={async (id: string) => {
                try {
                  await trashSession(id, "", "", "");
                  closeTab(id);
                  await sync.refreshTree();
                } catch (e) {
                  toastError(String(e));
                }
              }}
            />
          </Show>
          <Show when={activeView() === "settings"}>
            <SettingsPanel />
          </Show>
          <Show when={activeView() === "trash"}>
            <TrashView onRefreshTree={sync.refreshTree} />
          </Show>
          <Show when={activeView() === "favorites"}>
            <FavoritesView onOpenSession={openSession} />
          </Show>
          <Show when={activeView() === "blocked"}>
            <BlockedView onRefreshTree={sync.refreshTree} />
          </Show>
          <Show when={showExplorer()}>
            <EditorArea
              tabs={openTabs()}
              activeTabId={activeTabId()}
              onTabSelect={setActiveTabId}
              onTabClose={closeTab}
              onCloseAllTabs={closeAllTabs}
              onCloseOtherTabs={closeOtherTabs}
              onCloseTabsToRight={closeTabsToRight}
              onRefreshTree={sync.refreshTree}
              tree={filteredTree()}
              onOpenSession={openSession}
            />
          </Show>
        </div>
        <StatusBar
          sessionCount={sessionCount()}
          providerCount={filteredTree().length}
          isIndexing={isLoading()}
        />
        <KeyboardOverlay
          show={showKeyboardOverlay()}
          onClose={() => setShowKeyboardOverlay(false)}
        />
        <ToastContainer />
      </div>
    </ErrorBoundary>
  );
}
