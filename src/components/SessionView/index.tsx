import {
  createSignal,
  createEffect,
  createMemo,
  For,
  Show,
  on,
  onMount,
  onCleanup,
} from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  SessionRef,
  SessionMeta,
  Message,
  MessageRole,
} from "../../lib/types";
import { getProvider } from "../../lib/provider-registry";
import {
  getSessionDetail,
  trashSession,
  resumeSession,
  isFavorite,
  toggleFavorite,
} from "../../lib/tauri";
import { useI18n } from "../../i18n/index";
import { MessageBubble } from "../MessageBubble";
import { MergedToolRow } from "../MergedToolRow";
import { ConfirmDialog } from "../ConfirmDialog";
import { ExportDialog } from "../ExportDialog";
import { terminalApp } from "../../stores/settings";
import { toast, toastError } from "../../stores/toast";
import { errorMessage } from "../../lib/errors";
import { favoriteVersion, bumpFavoriteVersion } from "../../stores/favorites";
import { processMessages } from "./hooks";
import { SessionToolbar } from "./SessionToolbar";
import { SessionSearch } from "./SessionSearch";

export function SessionView(props: {
  session: SessionRef;
  onRefreshTree: () => void;
  onCloseTab: (id: string) => void;
}) {
  const { t } = useI18n();
  const [messages, setMessages] = createSignal<Message[]>([]);
  const processedEntries = createMemo(() => processMessages(messages()));
  const BATCH_SIZE = 80;
  const LOAD_MORE_THRESHOLD = 1;
  const [visibleCount, setVisibleCount] = createSignal(BATCH_SIZE);
  const [hiddenRoles, setHiddenRoles] = createSignal<Set<MessageRole>>(
    new Set(),
  );
  const [sessionSearch, setSessionSearch] = createSignal("");
  const [searchBarOpen, setSearchBarOpen] = createSignal(false);
  const [searchMatchIdx, setSearchMatchIdx] = createSignal(0);
  // Apply role filtering
  const filteredEntries = createMemo(() => {
    const hidden = hiddenRoles();
    if (hidden.size === 0) return processedEntries();
    return processedEntries().filter((e) => {
      if (e.type === "time-sep") return true;
      if (e.type === "merged-tools") return !hidden.has("tool");
      return !hidden.has(e.msg.role);
    });
  });

  // Role counts for filter toolbar
  const roleCounts = createMemo(() => {
    const counts: Record<string, number> = {
      user: 0,
      assistant: 0,
      tool: 0,
      system: 0,
    };
    for (const e of processedEntries()) {
      if (e.type === "message")
        counts[e.msg.role] = (counts[e.msg.role] || 0) + 1;
      else if (e.type === "merged-tools") counts.tool += e.messages.length;
    }
    return counts;
  });

  // Reversed for column-reverse layout: newest first in DOM = visually at bottom
  // When search is active, show all entries so DOM mark count matches actual matches
  const visibleEntries = createMemo(() => {
    const all = filteredEntries();
    const isSearching = sessionSearch().trim().length > 0;
    const count = isSearching ? all.length : visibleCount();
    const slice = count >= all.length ? all : all.slice(all.length - count);
    return [...slice].reverse();
  });
  const hasMore = createMemo(() => visibleCount() < filteredEntries().length);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [meta, setMeta] = createSignal<SessionMeta>({
    ...props.session,
    source_path: props.session.source_path ?? "",
    project_path: props.session.project_path ?? "",
    created_at: 0,
    updated_at: 0,
    message_count: 0,
    file_size_bytes: 0,
  });
  let loadVersion = 0;

  createEffect(
    on(
      () => props.session.id,
      async (sessionId) => {
        const version = ++loadVersion;
        setLoading(true);
        setError(null);
        setMessages([]);
        setVisibleCount(BATCH_SIZE);
        try {
          const detail = await getSessionDetail(sessionId);
          // Discard result if a newer load was triggered
          if (version !== loadVersion) return;
          setMeta(detail.meta);
          setMessages(detail.messages);
        } catch (e) {
          if (version !== loadVersion) return;
          setError(errorMessage(e));
        } finally {
          if (version === loadVersion) setLoading(false);
        }
      },
    ),
  );

  function toggleRole(role: MessageRole) {
    setHiddenRoles((prev) => {
      const next = new Set(prev);
      if (next.has(role)) next.delete(role);
      else next.add(role);
      return next;
    });
  }

  let messagesRef: HTMLDivElement | undefined;
  let loadOlderDebounce: ReturnType<typeof setTimeout> | undefined;

  function loadOlderEntries() {
    if (!messagesRef || !hasMore()) return;
    // column-reverse: older entries append at the end of the DOM (visual top).
    // The browser preserves scroll position automatically — no anchor restore needed.
    setVisibleCount((count) => count + BATCH_SIZE);
  }

  function handleMessagesScroll(e: Event) {
    const target = e.currentTarget as HTMLDivElement;
    clearTimeout(loadOlderDebounce);

    // column-reverse: scrollTop=0 is bottom (newest). User scrolls up -> scrollTop
    // goes negative. We want to load more when user reaches the visual top.
    // Visual top = max negative scrollTop = -(scrollHeight - clientHeight).
    const atVisualTop =
      target.scrollHeight + target.scrollTop - target.clientHeight <=
      LOAD_MORE_THRESHOLD;

    if (atVisualTop) {
      loadOlderDebounce = setTimeout(() => {
        if (!messagesRef) return;
        const stillAtTop =
          messagesRef.scrollHeight +
            messagesRef.scrollTop -
            messagesRef.clientHeight <=
          LOAD_MORE_THRESHOLD;
        if (stillAtTop) {
          loadOlderEntries();
        }
      }, 80);
    }
  }

  // Global keyboard shortcut listeners — must be inside lifecycle hooks
  const onResume = () => handleResume();
  const onExport = () => setShowExportDialog(true);
  const onFavorite = () => handleToggleFavorite();
  const onWatch = () => setWatching((v) => !v);
  const onDelete = () => setShowDeleteConfirm(true);
  const onSessionSearch = () => {
    setSearchBarOpen(true);
    requestAnimationFrame(() => {
      (
        document.querySelector(".session-search-input") as HTMLInputElement
      )?.focus();
    });
  };

  onMount(() => {
    document.addEventListener("cc-session:resume", onResume);
    document.addEventListener("cc-session:export", onExport);
    document.addEventListener("cc-session:favorite", onFavorite);
    document.addEventListener("cc-session:watch", onWatch);
    document.addEventListener("cc-session:delete", onDelete);
    document.addEventListener("cc-session:session-search", onSessionSearch);
  });

  onCleanup(() => {
    clearTimeout(loadOlderDebounce);
    clearTimeout(watchDebounce);
    unwatchFn?.();
    document.removeEventListener("cc-session:resume", onResume);
    document.removeEventListener("cc-session:export", onExport);
    document.removeEventListener("cc-session:favorite", onFavorite);
    document.removeEventListener("cc-session:watch", onWatch);
    document.removeEventListener("cc-session:delete", onDelete);
    document.removeEventListener("cc-session:session-search", onSessionSearch);
  });

  // column-reverse: scrollTop=0 naturally shows newest messages. No scroll-to-bottom needed.

  // Auto-load more if content doesn't fill the viewport
  createEffect(() => {
    visibleEntries();
    if (loading() || !hasMore() || !messagesRef) {
      return;
    }

    if (
      messagesRef.scrollHeight <=
      messagesRef.clientHeight + LOAD_MORE_THRESHOLD
    ) {
      requestAnimationFrame(() => {
        loadOlderEntries();
      });
    }
  });

  const [showDeleteConfirm, setShowDeleteConfirm] = createSignal(false);
  const [showExportDialog, setShowExportDialog] = createSignal(false);
  const [starred, setStarred] = createSignal(false);
  const [watching, setWatching] = createSignal(false);

  // Live watch: re-fetch session when file changes
  let unwatchFn: UnlistenFn | undefined;
  let watchDebounce: ReturnType<typeof setTimeout> | undefined;

  async function reloadSession() {
    try {
      const detail = await getSessionDetail(props.session.id);
      const oldCount = messages().length;
      setMeta(detail.meta);
      setMessages(detail.messages);
      // Auto-scroll to newest if new messages arrived (column-reverse: bottom = scrollTop 0)
      if (detail.messages.length > oldCount) {
        requestAnimationFrame(() => {
          messagesRef?.scrollTo({ top: 0, behavior: "smooth" });
        });
      }
    } catch (e) {
      console.warn("live watch reload failed:", e);
    }
  }

  let pollTimer: ReturnType<typeof setInterval> | undefined;

  createEffect(
    on(watching, async (isWatching) => {
      // Cleanup previous listener & polling
      clearTimeout(watchDebounce);
      clearInterval(pollTimer);
      pollTimer = undefined;
      unwatchFn?.();
      unwatchFn = undefined;

      if (isWatching) {
        const activeSourcePath =
          meta().source_path || props.session.source_path;
        const providerDef = getProvider(meta().provider);

        if (providerDef.watchStrategy === "poll") {
          pollTimer = setInterval(reloadSession, providerDef.watchDebounceMs);
        } else {
          // File-based providers: use FS events
          unwatchFn = await listen<string[]>("sessions-changed", (event) => {
            const changedPaths = event.payload ?? [];
            if (!activeSourcePath) return;

            let matched: boolean;
            if (providerDef.watchMatchPrefix) {
              // Gemini: match by project directory prefix
              // (strip last 2 path segments: /chats/session-id.json → project dir)
              const dir = activeSourcePath.replace(/\/[^/]+\/[^/]+$/, "");
              matched = changedPaths.some((p) => p.startsWith(dir));
            } else {
              matched = changedPaths.includes(activeSourcePath);
            }
            if (!matched) return;

            clearTimeout(watchDebounce);
            watchDebounce = setTimeout(
              reloadSession,
              providerDef.watchDebounceMs,
            );
          });
        }
      }
    }),
  );

  onCleanup(() => {
    clearTimeout(watchDebounce);
    clearInterval(pollTimer);
    pollTimer = undefined;
    unwatchFn?.();
  });

  // Re-check favorite when favorite version bumps
  createEffect(
    on(
      () => favoriteVersion(),
      async () => {
        try {
          const fav = await isFavorite(props.session.id);
          setStarred(fav);
        } catch {
          setStarred(false);
        }
      },
    ),
  );

  // Sync title from props when it changes (e.g. after rename via syncTabsWithTree)
  createEffect(
    on(
      () => props.session.title,
      (newTitle) => {
        setMeta((prev) => ({ ...prev, title: newTitle }));
      },
    ),
  );

  const handleToggleFavorite = async () => {
    try {
      const newState = await toggleFavorite(props.session.id);
      setStarred(newState);
      bumpFavoriteVersion();
      toast(t(newState ? "toast.favoriteAdded" : "toast.favoriteRemoved"));
    } catch (_e) {
      toastError(t("toast.favoriteFailed"));
    }
  };

  const handleCopy = async () => {
    const text = messages()
      .map((m) => `[${m.role}] ${m.content}`)
      .join("\n\n");
    try {
      await navigator.clipboard.writeText(text);
      toast(t("toast.copied"));
    } catch {
      toastError(t("toast.copyFailed"));
    }
  };

  const handleDelete = async () => {
    try {
      await trashSession(props.session.id);
      setShowDeleteConfirm(false);
      props.onCloseTab(props.session.id);
      props.onRefreshTree();
      toast(t("toast.trashed"));
    } catch (_e) {
      setShowDeleteConfirm(false);
      toastError(t("toast.trashFailed"));
    }
  };

  const handleResume = async () => {
    try {
      await resumeSession(props.session.id, terminalApp());
      toast(t("toast.resumed"));
    } catch (_e) {
      toastError(t("toast.resumeFailed"));
    }
  };

  return (
    <div class="session-view">
      <SessionToolbar
        meta={meta}
        messages={messages}
        processedEntries={processedEntries}
        watching={watching}
        starred={starred}
        onToggleWatch={() => setWatching((v) => !v)}
        onToggleFavorite={handleToggleFavorite}
        onResume={handleResume}
        onExport={() => setShowExportDialog(true)}
        onCopy={handleCopy}
        onDelete={() => setShowDeleteConfirm(true)}
      />

      {/* Filter toolbar — only show roles that have messages */}
      <div class="filter-toolbar">
        <For
          each={(
            ["user", "assistant", "tool", "system"] as MessageRole[]
          ).filter((r) => (roleCounts()[r] || 0) > 0)}
        >
          {(role) => (
            <button
              class={`filter-btn${hiddenRoles().has(role) ? "" : " active"}`}
              onClick={() => toggleRole(role)}
            >
              {role === "user"
                ? t("session.filterUser")
                : role === "assistant"
                  ? t("session.filterAssistant")
                  : role === "tool"
                    ? t("session.filterTool")
                    : t("session.filterSystem")}{" "}
              ({roleCounts()[role]})
            </button>
          )}
        </For>
      </div>

      {/* In-session search bar */}
      <Show when={searchBarOpen()}>
        <SessionSearch
          sessionSearch={sessionSearch}
          setSessionSearch={setSessionSearch}
          searchMatchIdx={searchMatchIdx}
          setSearchMatchIdx={setSearchMatchIdx}
          setSearchBarOpen={setSearchBarOpen}
          messagesRef={messagesRef}
        />
      </Show>

      {/* Content */}
      <Show when={loading()}>
        <div class="session-loading">
          <div class="spinner" />
          <span>{t("session.loading")}</span>
        </div>
      </Show>

      <Show when={error()}>
        <div class="session-error">{error()}</div>
      </Show>

      <Show when={!loading() && !error()}>
        <div
          class="session-messages"
          ref={messagesRef}
          onScroll={handleMessagesScroll}
        >
          <For each={visibleEntries()}>
            {(entry) => {
              if (entry.type === "time-sep") {
                return (
                  <div class="session-entry" data-entry-key={entry.key}>
                    <div class="msg-time-separator">{entry.time}</div>
                  </div>
                );
              }
              if (entry.type === "merged-tools") {
                return (
                  <div class="session-entry" data-entry-key={entry.key}>
                    <MergedToolRow
                      tools={entry.tools}
                      messages={entry.messages}
                      highlightTerm={sessionSearch()}
                    />
                  </div>
                );
              }
              return (
                <div class="session-entry" data-entry-key={entry.key}>
                  <MessageBubble
                    message={entry.msg}
                    provider={meta().provider}
                    highlightTerm={sessionSearch()}
                  />
                </div>
              );
            }}
          </For>
          <Show when={messages().length === 0}>
            <div class="session-empty-messages">{t("session.noMessages")}</div>
          </Show>
        </div>
      </Show>

      <ConfirmDialog
        open={showDeleteConfirm()}
        title={t("confirm.deleteTitle")}
        message={t("confirm.deleteMsg")}
        confirmLabel={t("confirm.confirm")}
        onConfirm={handleDelete}
        onCancel={() => setShowDeleteConfirm(false)}
        danger={true}
      />

      <ExportDialog
        open={showExportDialog()}
        session={meta()}
        onClose={() => setShowExportDialog(false)}
      />
    </div>
  );
}
