import { Show, For, createSignal, createResource, createEffect, on, onMount, onCleanup } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SessionMeta, TreeNode } from "../lib/types";
import { listRecentSessions } from "../lib/tauri";
import { useI18n } from "../i18n/index";
import { isPathBlocked } from "../stores/settings";
import { TabBar } from "./TabBar";
import { SessionView } from "./SessionView";
import { ProviderIcon } from "../lib/icons";
import { isMac } from "../lib/platform";

export function EditorArea(props: {
  tabs: SessionMeta[];
  activeTabId: string | null;
  onTabSelect: (id: string) => void;
  onTabClose: (id: string) => void;
  onCloseAllTabs: () => void;
  onCloseOtherTabs: (keepId: string) => void;
  onCloseTabsToRight: (fromId: string) => void;
  onRefreshTree: () => void;
  tree: TreeNode[];
  onOpenSession: (session: SessionMeta) => void;
}) {
  const { t } = useI18n();
  const activeSession = () => props.tabs.find((tab) => tab.id === props.activeTabId) ?? null;

  // Refresh trigger: bumped on mount and whenever sessions change
  const [recentVersion, setRecentVersion] = createSignal(0);
  const [recentSessions] = createResource(recentVersion, () =>
    listRecentSessions(20)
      .catch(() => [])
      .then((list) => list.filter((s) => !isPathBlocked(s.project_path)).slice(0, 10)),
  );

  // Refresh recent sessions when tree changes (covers coldStart, syncFromDisk, manual refresh, all providers)
  createEffect(
    on(
      () => props.tree,
      () => setRecentVersion((v) => v + 1),
      { defer: true },
    ),
  );

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    listen<void>("sessions-changed", () => setRecentVersion((v) => v + 1)).then((fn) => {
      unlisten = fn;
    });
    onCleanup(() => unlisten?.());
  });

  const modKey = isMac ? "\u2318" : "Ctrl+";

  return (
    <div class="editor-area">
      <Show
        when={props.tabs.length > 0}
        fallback={
          <div class="editor-empty">
            <div class="editor-empty-icon">
              <svg width="48" height="48" fill="none" stroke="currentColor" stroke-width="1" viewBox="0 0 24 24">
                <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
              </svg>
            </div>
            <Show when={recentSessions() && recentSessions()!.length > 0}>
              <div class="editor-empty-recent">
                <p class="editor-empty-label">{t("editor.recentSessions")}</p>
                <For each={recentSessions()}>
                  {(session) => (
                    <button class="editor-empty-session" onClick={() => props.onOpenSession(session)}>
                      <span class="provider-dot provider-logo" style={{ color: `var(--${session.provider})` }}>
                        <ProviderIcon provider={session.provider} />
                      </span>
                      <div class="editor-empty-session-info">
                        <span class="editor-empty-session-title">
                          {session.title}
                          <Show when={session.is_sidechain}>
                            <span class="session-sidechain-badge">⤷</span>
                          </Show>
                        </span>
                        <span class="editor-empty-session-path">
                          {session.project_path ? session.project_path.split("/").slice(-2).join("/") : ""}
                        </span>
                      </div>
                    </button>
                  )}
                </For>
              </div>
            </Show>
            <Show when={!recentSessions() || recentSessions()!.length === 0}>
              <p class="editor-empty-text">{t("editor.emptyHint")}</p>
            </Show>
            <div class="editor-empty-shortcuts">
              <span class="editor-shortcut-hint">
                <kbd>⇧{modKey}F</kbd> {t("keyboard.search")}
              </span>
              <span class="editor-shortcut-hint">
                <kbd>{modKey}1-9</kbd> {t("keyboard.switchTab")}
              </span>
            </div>
          </div>
        }
      >
        <TabBar
          tabs={props.tabs}
          activeTabId={props.activeTabId}
          onTabSelect={props.onTabSelect}
          onTabClose={props.onTabClose}
          onCloseAllTabs={props.onCloseAllTabs}
          onCloseOtherTabs={props.onCloseOtherTabs}
          onCloseTabsToRight={props.onCloseTabsToRight}
        />
        <div class="editor-content">
          <Show when={activeSession()}>
            {(session) => (
              <SessionView session={session()} onRefreshTree={props.onRefreshTree} onCloseTab={props.onTabClose} />
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
}
