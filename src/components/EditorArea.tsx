import {
  Show,
  For,
  Index,
  createMemo,
  createSignal,
  createResource,
  createEffect,
  on,
  onMount,
  onCleanup,
} from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SessionRef, TreeNode } from "../lib/types";
import {
  listRecentSessions,
  getChildSessions,
  invokeWithFallback,
} from "../lib/tauri";
import { useI18n } from "../i18n/index";
import { isPathBlocked } from "../stores/settings";
import { groups } from "../stores/editorGroups";
import { errorMessage } from "../lib/errors";
import { formatTimestamp } from "../lib/formatters";
import { TabBar } from "./TabBar";
import { SessionView } from "./SessionView";
import { ProviderIcon } from "../lib/icons";
import { isMac } from "../lib/platform";

export function EditorArea(props: {
  groupId: string;
  tabs: SessionRef[];
  activeTabId: string | null;
  previewTabId: string | null;
  isFocused: boolean;
  flexBasis: number;
  onFocus: () => void;
  onTabSelect: (id: string) => void;
  onTabClose: (id: string) => void;
  onCloseAllTabs: () => void;
  onCloseOtherTabs: (keepId: string) => void;
  onCloseTabsToRight: (fromId: string) => void;
  onSplitToRight: (sessionId: string) => void;
  onPinTab: (sessionId: string) => void;
  onRefreshTree: () => void;
  tree: TreeNode[];
  onOpenSession: (session: SessionRef) => void;
}) {
  const { t, locale } = useI18n();
  // Refresh trigger: bumped on mount and whenever sessions change
  const [recentVersion, setRecentVersion] = createSignal(0);
  const [recentSessions] = createResource(recentVersion, async () => {
    const list = await listRecentSessions(100);
    return list
      .filter((s) => !isPathBlocked(s.project_path) && !s.is_sidechain)
      .slice(0, 10);
  });
  const recentSessionsError = createMemo(() =>
    recentSessions.error ? errorMessage(recentSessions.error) : null,
  );
  // Child counts per session for badge display
  const [childCounts, setChildCounts] = createSignal<Record<string, number>>(
    {},
  );
  createEffect(
    on(
      () => recentSessions(),
      async (sessions) => {
        if (!sessions) return;
        const counts: Record<string, number> = {};
        await Promise.all(
          sessions.map(async (s) => {
            const children = await invokeWithFallback(
              getChildSessions(s.id),
              [],
              `load child sessions for recent session ${s.id}`,
            );
            if (children.length > 0) counts[s.id] = children.length;
          }),
        );
        setChildCounts(counts);
      },
      { defer: true },
    ),
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
    listen<void>("sessions-changed", () => setRecentVersion((v) => v + 1)).then(
      (fn) => {
        unlisten = fn;
      },
    );
    onCleanup(() => unlisten?.());
  });

  const modKey = isMac ? "\u2318" : "Ctrl+";

  return (
    <div
      class={`editor-area${props.isFocused ? " focused" : ""}`}
      style={{ "flex-basis": `${props.flexBasis}%` }}
      onClick={() => props.onFocus()}
    >
      <Show
        when={props.tabs.length > 0}
        fallback={
          <Show when={groups().length === 1}>
            <div class="editor-empty">
              <div class="editor-empty-icon">
                <svg
                  width="48"
                  height="48"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1"
                  viewBox="0 0 24 24"
                >
                  <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                </svg>
              </div>
              <Show when={recentSessions() && recentSessions()!.length > 0}>
                <div class="editor-empty-recent">
                  <p class="editor-empty-label">{t("editor.recentSessions")}</p>
                  <For each={recentSessions()}>
                    {(session) => (
                      <button
                        class="editor-empty-session"
                        onClick={() => props.onOpenSession(session)}
                      >
                        <span
                          class="provider-dot provider-logo"
                          style={{ color: `var(--${session.provider})` }}
                        >
                          <ProviderIcon provider={session.provider} />
                        </span>
                        <div class="editor-empty-session-info">
                          <span class="editor-empty-session-title">
                            {session.title}
                          </span>
                          <span class="editor-empty-session-meta">
                            <span class="editor-empty-session-path">
                              {session.project_name || ""}
                            </span>
                            <Show when={session.model}>
                              <span class="editor-empty-session-model">
                                {session.model}
                              </span>
                            </Show>
                            <Show when={childCounts()[session.id]}>
                              <span class="editor-empty-session-agents">
                                🤖 {childCounts()[session.id]}
                              </span>
                            </Show>
                          </span>
                        </div>
                        <span class="editor-empty-session-time">
                          {formatTimestamp(session.updated_at, locale())}
                        </span>
                      </button>
                    )}
                  </For>
                </div>
              </Show>
              <Show when={recentSessionsError()}>
                <p class="editor-empty-text">{recentSessionsError()}</p>
              </Show>
              <Show
                when={
                  !recentSessions.loading &&
                  !recentSessionsError() &&
                  (!recentSessions() || recentSessions()!.length === 0)
                }
              >
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
          </Show>
        }
      >
        <TabBar
          groupId={props.groupId}
          tabs={props.tabs}
          activeTabId={props.activeTabId}
          previewTabId={props.previewTabId}
          onTabSelect={props.onTabSelect}
          onTabClose={props.onTabClose}
          onCloseAllTabs={props.onCloseAllTabs}
          onCloseOtherTabs={props.onCloseOtherTabs}
          onCloseTabsToRight={props.onCloseTabsToRight}
          onSplitToRight={props.onSplitToRight}
          onPinTab={props.onPinTab}
        />
        <div class="editor-content">
          <Index each={props.tabs}>
            {(session) => (
              <div
                class="editor-tab-pane"
                style={{
                  display: session().id === props.activeTabId ? "flex" : "none",
                  flex: "1",
                  "flex-direction": "column",
                  "min-height": "0",
                }}
              >
                <Show when={session().id} keyed>
                  {(_id) => (
                    <SessionView
                      session={session()}
                      onRefreshTree={props.onRefreshTree}
                      onCloseTab={props.onTabClose}
                    />
                  )}
                </Show>
              </div>
            )}
          </Index>
        </div>
      </Show>
    </div>
  );
}
