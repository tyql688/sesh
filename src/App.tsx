import { createSignal, createMemo, onMount, onCleanup, Show } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ActivityBar } from "./components/ActivityBar";
import { Explorer } from "./components/Explorer";
import { EditorArea } from "./components/EditorArea";
import { StatusBar } from "./components/StatusBar";
import { SearchPanel } from "./components/SearchPanel";
import { SettingsPanel } from "./components/SettingsPanel";
import { TrashView } from "./components/TrashView";

import { FavoritesView } from "./components/FavoritesView";
import { BlockedView } from "./components/BlockedView";
import { KeyboardOverlay } from "./components/KeyboardOverlay";
import { ToastContainer } from "./components/ToastContainer";
import { reindex, syncSources, getTree, getSessionCount, trashSession } from "./lib/tauri";
import { disabledProviders } from "./stores/settings";
import type { TreeNode, SessionMeta } from "./lib/types";
import "./styles/index.css";

export default function App() {
  const [tree, setTree] = createSignal<TreeNode[]>([]);
  const [sessionCount, setSessionCount] = createSignal(0);
  const [activeView, setActiveView] = createSignal("explorer");
  const [openTabs, setOpenTabs] = createSignal<SessionMeta[]>([]);
  const [activeTabId, setActiveTabId] = createSignal<string | null>(null);
  const [isLoading, setIsLoading] = createSignal(true);
  const [showKeyboardOverlay, setShowKeyboardOverlay] = createSignal(false);
  let syncInFlight = false;
  let pendingFullSync = false;
  const pendingChangedPaths = new Set<string>();
  const debouncedChangedPaths = new Set<string>();

  let unlistenWatcher: UnlistenFn | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  onMount(async () => {
    void syncFromDisk({ showSpinner: true });

    document.addEventListener("keydown", handleGlobalKeyDown);

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
        void syncFromDisk({ changedPaths });
      }, 500);
    });
  });

  function handleGlobalKeyDown(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;

    // Cmd+/ : Toggle keyboard shortcuts overlay
    if (mod && e.key === "/") {
      e.preventDefault();
      setShowKeyboardOverlay((prev) => !prev);
      return;
    }

    // Unmodified ? when not in an input: show keyboard shortcuts
    if (
      e.key === "?" &&
      !mod &&
      !e.altKey &&
      !(document.activeElement instanceof HTMLInputElement) &&
      !(document.activeElement instanceof HTMLTextAreaElement) &&
      !document.activeElement?.hasAttribute("contenteditable")
    ) {
      e.preventDefault();
      setShowKeyboardOverlay(true);
      return;
    }

    // Cmd+Shift+W / Ctrl+Shift+W: Close all tabs
    if (mod && e.shiftKey && (e.key === "w" || e.key === "W")) {
      e.preventDefault();
      closeAllTabs();
      return;
    }

    // Cmd+W / Ctrl+W: Close active tab
    if (mod && e.key === "w") {
      e.preventDefault();
      const id = activeTabId();
      if (id) closeTab(id);
      return;
    }

    // Cmd+1-9: Switch to tab by index
    if (mod && e.key >= "1" && e.key <= "9") {
      e.preventDefault();
      const idx = parseInt(e.key) - 1;
      const tabs = openTabs();
      if (idx < tabs.length) {
        setActiveTabId(tabs[idx].id);
      }
      return;
    }

    // Escape: Close keyboard overlay or search dropdown
    if (e.key === "Escape") {
      if (showKeyboardOverlay()) {
        setShowKeyboardOverlay(false);
        return;
      }
      const searchEl = document.querySelector<HTMLElement>("[data-focus-search]");
      if (searchEl && document.activeElement?.closest("[data-focus-search]")) {
        (document.activeElement as HTMLElement)?.blur();
      }
      return;
    }

    // Cmd+] or Ctrl+Tab: Next tab
    if ((e.metaKey && e.key === "]") || (e.ctrlKey && e.key === "Tab" && !e.shiftKey)) {
      e.preventDefault();
      const tabs = openTabs();
      const currentId = activeTabId();
      if (tabs.length > 1 && currentId) {
        const idx = tabs.findIndex((t) => t.id === currentId);
        const nextIdx = (idx + 1) % tabs.length;
        setActiveTabId(tabs[nextIdx].id);
      }
      return;
    }

    // Cmd+[ or Ctrl+Shift+Tab: Previous tab
    if ((e.metaKey && e.key === "[") || (e.ctrlKey && e.key === "Tab" && e.shiftKey)) {
      e.preventDefault();
      const tabs = openTabs();
      const currentId = activeTabId();
      if (tabs.length > 1 && currentId) {
        const idx = tabs.findIndex((t) => t.id === currentId);
        const prevIdx = (idx - 1 + tabs.length) % tabs.length;
        setActiveTabId(tabs[prevIdx].id);
      }
      return;
    }

    // Cmd+, : Open settings
    if (mod && e.key === ",") {
      e.preventDefault();
      setActiveView("settings");
      return;
    }

    // Cmd+Shift+F: Focus global search
    if (mod && e.shiftKey && (e.key === "f" || e.key === "F")) {
      e.preventDefault();
      const searchEl = document.querySelector<HTMLElement>("[data-focus-search]");
      if (searchEl) {
        (searchEl as HTMLElement & { __focusInput?: () => void }).__focusInput?.();
      }
      return;
    }

    // Cmd+R: Refresh index
    if (mod && !e.shiftKey && e.key === "r") {
      e.preventDefault();
      void syncFromDisk({ showSpinner: true });
      return;
    }

    // Session-scoped shortcuts (only when a tab is active)
    if (!activeTabId()) return;

    // Cmd+F: Find in session
    if (mod && e.key === "f") {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent("cc-session:session-search"));
      return;
    }

    // Cmd+Shift+R: Resume session
    if (mod && e.shiftKey && (e.key === "r" || e.key === "R")) {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent("cc-session:resume"));
      return;
    }

    // Cmd+Shift+E: Export session
    if (mod && e.shiftKey && (e.key === "e" || e.key === "E")) {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent("cc-session:export"));
      return;
    }

    // Cmd+B: Toggle favorite
    if (mod && e.key === "b") {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent("cc-session:favorite"));
      return;
    }

    // Cmd+L: Toggle live watch
    if (mod && e.key === "l") {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent("cc-session:watch"));
      return;
    }

    // Cmd+Backspace: Delete session
    if (mod && e.key === "Backspace") {
      e.preventDefault();
      document.dispatchEvent(new CustomEvent("cc-session:delete"));
      return;
    }
  }

  onCleanup(() => {
    document.removeEventListener("keydown", handleGlobalKeyDown);
    unlistenWatcher?.();
    clearTimeout(debounceTimer);
    debouncedChangedPaths.clear();
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

  async function refreshTree() {
    const [treeData, count] = await Promise.all([getTree(), getSessionCount()]);
    setTree(treeData);
    setSessionCount(count);
    // Sync open tab titles with latest tree data
    syncTabsWithTree(treeData);
  }

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
      })
    );
  }

  async function syncFromDisk(options?: { changedPaths?: string[]; showSpinner?: boolean }) {
    const changedPaths = options?.changedPaths?.filter((path) => path.length > 0) ?? [];
    const showSpinner = options?.showSpinner ?? false;

    if (syncInFlight) {
      if (changedPaths.length > 0 && !pendingFullSync) {
        for (const path of changedPaths) pendingChangedPaths.add(path);
      } else {
        pendingFullSync = true;
      }
      return;
    }

    syncInFlight = true;
    if (showSpinner) {
      setIsLoading(true);
    }

    try {
      if (changedPaths.length > 0) {
        await syncSources(changedPaths);
      } else {
        await reindex();
      }
      await refreshTree();
    } catch (e) {
      console.warn("Failed to synchronize sessions:", e);
    } finally {
      syncInFlight = false;
      if (showSpinner) {
        setIsLoading(false);
      }
      if (pendingFullSync) {
        pendingFullSync = false;
        pendingChangedPaths.clear();
        void syncFromDisk({ showSpinner });
      } else if (pendingChangedPaths.size > 0) {
        const queuedPaths = [...pendingChangedPaths];
        pendingChangedPaths.clear();
        void syncFromDisk({ changedPaths: queuedPaths });
      }
    }
  }

  const filteredTree = createMemo(() =>
    tree().filter((node) => !disabledProviders().includes(node.id as "claude" | "codex" | "gemini"))
  );
  const showExplorer = createMemo(() => {
    const v = activeView();
    return v !== "settings" && v !== "trash" && v !== "blocked";
  });
  const showExplorerTree = createMemo(() => {
    const v = activeView();
    return v !== "settings" && v !== "trash" && v !== "favorites" && v !== "blocked";
  });

  return (
    <div class="app-layout">
      <div
        class="titlebar"
        onMouseDown={(e) => {
          if (e.buttons !== 1) return;
          if ((e.target as HTMLElement).closest("input, button, .search-panel")) return;
          e.preventDefault();
          if (e.detail === 2) {
            getCurrentWindow().toggleMaximize();
          } else {
            getCurrentWindow().startDragging();
          }
        }}
      >
        <div class="titlebar-center">
          <span class="app-name"><span class="app-name-bracket">&lt;</span>cc-session<span class="app-name-bracket">/&gt;</span></span>
        </div>
        <div class="titlebar-right">
          <SearchPanel onOpenSession={openSession} />
        </div>
      </div>
      <div class="main-layout">
        <ActivityBar activeView={activeView()} onViewChange={setActiveView} />
        <Show when={showExplorerTree()}>
          <Explorer
            tree={filteredTree()}
            isLoading={isLoading()}
            activeSessionId={activeTabId()}
            onOpenSession={openSession}
            onRefreshTree={refreshTree}
            onDeleteSession={async (id: string) => {
              try {
                await trashSession(id, "", "", "");
                closeTab(id);
                await refreshTree();
              } catch (e) {
                console.warn("Failed to trash session:", e);
              }
            }}
          />
        </Show>
        <Show when={activeView() === "settings"}>
          <SettingsPanel />
        </Show>
        <Show when={activeView() === "trash"}>
          <TrashView onRefreshTree={refreshTree} />
        </Show>
        <Show when={activeView() === "favorites"}>
          <FavoritesView onOpenSession={openSession} />
        </Show>
        <Show when={activeView() === "blocked"}>
          <BlockedView onRefreshTree={refreshTree} />
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
            onRefreshTree={refreshTree}
            tree={filteredTree()}
            onOpenSession={openSession}
          />
        </Show>
      </div>
      <StatusBar sessionCount={sessionCount()} providerCount={filteredTree().length} isIndexing={isLoading()} />
      <KeyboardOverlay show={showKeyboardOverlay()} onClose={() => setShowKeyboardOverlay(false)} />
      <ToastContainer />
    </div>
  );
}
