import { createSignal, createEffect, createMemo, For, Show, on, onMount, onCleanup } from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SessionMeta, Message, MessageRole } from "../lib/types";
import { getSessionDetail, trashSession, resumeSession, isFavorite, toggleFavorite } from "../lib/tauri";
import { useI18n } from "../i18n/index";
import { getProviderLabel } from "../lib/providers";
import { MessageBubble } from "./MessageBubble";
import { MergedToolRow } from "./MergedToolRow";
import { ConfirmDialog } from "./ConfirmDialog";
import { ExportDialog } from "./ExportDialog";
import { terminalApp } from "../stores/settings";
import { toast, toastError } from "../stores/toast";
import { favoriteVersion, bumpFavoriteVersion } from "../stores/favorites";
import { parseTimestamp, formatTimeOnly, formatTimestamp, fmtK, formatFileSize } from "../lib/formatters";

type ProcessedEntry =
  | { key: string; type: "message"; msg: Message }
  | { key: string; type: "time-sep"; time: string }
  | { key: string; type: "merged-tools"; tools: string[]; messages: Message[] };

function processMessages(msgs: Message[]): ProcessedEntry[] {
  const entries: ProcessedEntry[] = [];
  let i = 0;

  while (i < msgs.length) {
    const msg = msgs[i];

    // Try to merge consecutive tool messages
    if (msg.role === "tool") {
      const toolGroup: Message[] = [msg];
      let j = i + 1;
      while (j < msgs.length && msgs[j].role === "tool") {
        toolGroup.push(msgs[j]);
        j++;
      }
      if (toolGroup.length > 1) {
        const toolNames = toolGroup
          .map((m) => m.tool_name)
          .filter((n): n is string => !!n && n.trim().length > 0);
        entries.push({
          key: `tools-${i}-${toolGroup[0].timestamp ?? "none"}`,
          type: "merged-tools",
          tools: toolNames,
          messages: toolGroup,
        });
      } else {
        entries.push({
          key: `msg-${i}-${msg.role}-${msg.timestamp ?? "none"}`,
          type: "message",
          msg,
        });
      }
      i = j;
      continue;
    }

    // Check time gap with previous message
    if (entries.length > 0) {
      const prevEntry = entries[entries.length - 1];
      let prevTs: number | null = null;
      if (prevEntry.type === "message") {
        prevTs = parseTimestamp(prevEntry.msg.timestamp);
      } else if (prevEntry.type === "merged-tools") {
        const lastTool = prevEntry.messages[prevEntry.messages.length - 1];
        prevTs = parseTimestamp(lastTool.timestamp);
      }
      const curTs = parseTimestamp(msg.timestamp);
      const TIME_GAP_THRESHOLD_MS = 5 * 60 * 1000; // 5 minutes
      if (prevTs && curTs && curTs - prevTs > TIME_GAP_THRESHOLD_MS) {
        entries.push({
          key: `sep-${i}-${curTs}`,
          type: "time-sep",
          time: formatTimeOnly(curTs),
        });
      }
    }

    entries.push({
      key: `msg-${i}-${msg.role}-${msg.timestamp ?? "none"}`,
      type: "message",
      msg,
    });
    i++;
  }

  return entries;
}

export function SessionView(props: {
  session: SessionMeta;
  onRefreshTree: () => void;
  onCloseTab: (id: string) => void;
}) {
  const { t, locale } = useI18n();
  const [messages, setMessages] = createSignal<Message[]>([]);
  const processedEntries = createMemo(() => processMessages(messages()));
  const BATCH_SIZE = 80;
  const LOAD_MORE_THRESHOLD = 1;
  const [visibleCount, setVisibleCount] = createSignal(BATCH_SIZE);
  const [hiddenRoles, setHiddenRoles] = createSignal<Set<MessageRole>>(new Set());
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

  // Search matches
  const searchMatches = createMemo(() => {
    const term = sessionSearch().toLowerCase().trim();
    if (!term) return [] as number[];
    const indices: number[] = [];
    filteredEntries().forEach((e, idx) => {
      if (e.type === "message" && e.msg.content.toLowerCase().includes(term)) {
        indices.push(idx);
      } else if (e.type === "merged-tools" && e.messages.some((m) => m.content.toLowerCase().includes(term))) {
        indices.push(idx);
      }
    });
    return indices;
  });

  // Total token usage across all messages
  const totalTokens = createMemo(() => {
    let input = 0, output = 0, cacheRead = 0;
    for (const e of processedEntries()) {
      const msgs = e.type === "message" ? [e.msg] : e.type === "merged-tools" ? e.messages : [];
      for (const m of msgs) {
        if (m.token_usage) {
          input += m.token_usage.input_tokens;
          output += m.token_usage.output_tokens;
          cacheRead += m.token_usage.cache_read_input_tokens;
        }
      }
    }
    return input + output > 0 ? { input, output, cacheRead } : null;
  });

  // Role counts for filter toolbar
  const roleCounts = createMemo(() => {
    const counts: Record<string, number> = { user: 0, assistant: 0, tool: 0, system: 0 };
    for (const e of processedEntries()) {
      if (e.type === "message") counts[e.msg.role] = (counts[e.msg.role] || 0) + 1;
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
  const [meta, setMeta] = createSignal<SessionMeta>(props.session);
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
          const detail = await getSessionDetail(
            sessionId,
            props.session.source_path,
            props.session.provider,
          );
          // Discard result if a newer load was triggered
          if (version !== loadVersion) return;
          setMeta(detail.meta);
          setMessages(detail.messages);
        } catch (e) {
          if (version !== loadVersion) return;
          setError(String(e));
        } finally {
          if (version === loadVersion) setLoading(false);
        }
      },
    ),
  );

  const providerLabel = () => {
    return getProviderLabel(meta().provider);
  };

  function toggleRole(role: MessageRole) {
    setHiddenRoles((prev) => {
      const next = new Set(prev);
      if (next.has(role)) next.delete(role);
      else next.add(role);
      return next;
    });
  }

  /** Get marks in visual order (top→bottom). Sort by position since column-reverse
   *  flips message order but not text order within each message. */
  function getMarksInVisualOrder(): Element[] {
    if (!messagesRef) return [];
    const marks = Array.from(messagesRef.querySelectorAll("mark.search-highlight"));
    marks.sort((a, b) => {
      const ra = a.getBoundingClientRect();
      const rb = b.getBoundingClientRect();

      return ra.top - rb.top || ra.left - rb.left;
    });

    return marks;
  }

  function navigateSearchMatch(delta: number) {
    const marks = getMarksInVisualOrder();
    if (marks.length === 0) return;
    // Remove previous active highlight
    messagesRef?.querySelector("mark.search-active")?.classList.remove("search-active");
    const newIdx = (searchMatchIdx() + delta + marks.length) % marks.length;
    setSearchMatchIdx(newIdx);
    const target = marks[newIdx];
    target.classList.add("search-active");
    target.scrollIntoView({ behavior: "smooth", block: "center" });
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

    // column-reverse: scrollTop=0 is bottom (newest). User scrolls up → scrollTop
    // goes negative. We want to load more when user reaches the visual top.
    // Visual top = max negative scrollTop = -(scrollHeight - clientHeight).
    const atVisualTop =
      target.scrollHeight + target.scrollTop - target.clientHeight <= LOAD_MORE_THRESHOLD;

    if (atVisualTop) {
      loadOlderDebounce = setTimeout(() => {
        if (!messagesRef) return;
        const stillAtTop =
          messagesRef.scrollHeight + messagesRef.scrollTop - messagesRef.clientHeight <=
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
      (document.querySelector(".session-search-input") as HTMLInputElement)?.focus();
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

    if (messagesRef.scrollHeight <= messagesRef.clientHeight + LOAD_MORE_THRESHOLD) {
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
      const detail = await getSessionDetail(
        props.session.id,
        props.session.source_path,
        props.session.provider,
      );
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

  createEffect(on(watching, async (isWatching) => {
    // Cleanup previous listener & polling
    clearTimeout(watchDebounce);
    clearInterval(pollTimer);
    pollTimer = undefined;
    unwatchFn?.();
    unwatchFn = undefined;

    if (isWatching) {
      const activeSourcePath = meta().source_path || props.session.source_path;
      const isDbSource = activeSourcePath?.endsWith(".db");

      if (isDbSource) {
        // SQLite-based providers (Cursor, OpenCode): use polling
        // FSEvents on macOS doesn't reliably detect SQLite WAL changes
        pollTimer = setInterval(reloadSession, 2000);
      } else {
        // File-based providers (Claude, Codex, Gemini): use FS events
        unwatchFn = await listen<string[]>("sessions-changed", (event) => {
          const changedPaths = event.payload ?? [];
          if (!activeSourcePath) return;

          const isGemini = meta().provider === "gemini";
          let matched = changedPaths.includes(activeSourcePath);
          if (!matched && isGemini) {
            const projectDir = activeSourcePath.replace(/\/chats\/.*$/, "");
            matched = changedPaths.some((p) => p.startsWith(projectDir));
          }
          if (!matched) return;

          clearTimeout(watchDebounce);
          const delay = isGemini ? 800 : 300;
          watchDebounce = setTimeout(reloadSession, delay);
        });
      }
    }
  }));

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
    } catch (e) {
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
      await trashSession(
        props.session.id,
        props.session.source_path,
        props.session.provider,
        props.session.title,
      );
      setShowDeleteConfirm(false);
      props.onCloseTab(props.session.id);
      props.onRefreshTree();
      toast(t("toast.trashed"));
    } catch (e) {
      setShowDeleteConfirm(false);
      toastError(t("toast.trashFailed"));
    }
  };

  const handleResume = async () => {
    try {
      await resumeSession(props.session.id, meta().provider, terminalApp());
      toast(t("toast.resumed"));
    } catch (e) {
      toastError(t("toast.resumeFailed"));
    }
  };

  return (
    <div class="session-view">
      {/* Header */}
      <div class="session-header">
        <div class="session-breadcrumb">
          <div class="breadcrumb-nav">
            <span class="breadcrumb-provider" style={{ color: `var(--${meta().provider})` }}>
              {providerLabel()}
            </span>
            <span class="breadcrumb-sep">&rsaquo;</span>
            <span class="breadcrumb-project">{meta().project_name || t("explorer.noProject")}</span>
          </div>
          <div class="breadcrumb-title">{meta().title}</div>
        </div>
        <div class="session-actions">
          <button
            class={`session-action-btn session-action-btn-icon${watching() ? " watching" : ""}`}
            onClick={() => setWatching((v) => !v)}
            title={watching() ? t("session.watchStop") : t("session.watchStart")}
          >
            {watching() ? "\u25C9" : "\u25CE"}
          </button>
          <button
            class={`session-action-btn session-action-btn-icon${starred() ? " starred" : ""}`}
            onClick={handleToggleFavorite}
            title={starred() ? t("session.favoriteRemove") : t("session.favoriteAdd")}
          >
            {starred() ? "\u2605" : "\u2606"}
          </button>
          <button class="session-action-btn primary" onClick={handleResume} title={t("session.resume")}>
            {t("session.resume")}
          </button>
          <button class="session-action-btn" onClick={() => setShowExportDialog(true)} title={t("session.export")}>
            {t("session.export")}
          </button>
          <button class="session-action-btn" onClick={handleCopy} title={t("session.copy")}>
            {t("session.copy")}
          </button>
          <button class="session-action-btn session-action-btn-danger" onClick={() => setShowDeleteConfirm(true)} title={t("session.delete")}>
            {t("session.delete")}
          </button>
        </div>
      </div>

      {/* Info bar */}
      <div class="session-info">
        <span>{t("session.created")}: {formatTimestamp(meta().created_at, locale())}</span>
        <span class="info-sep">&middot;</span>
        <span>{meta().message_count || messages().length} {t("session.messages")}</span>
        <span class="info-sep">&middot;</span>
        <span>{formatFileSize(meta().file_size_bytes)}</span>
        <Show when={totalTokens()}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-tokens" title={`Input: ${totalTokens()!.input.toLocaleString()}, Output: ${totalTokens()!.output.toLocaleString()}${totalTokens()!.cacheRead > 0 ? `, Cache hit: ${totalTokens()!.cacheRead.toLocaleString()}` : ""}`}>
            ↑{fmtK(totalTokens()!.input)} ↓{fmtK(totalTokens()!.output)} tokens
          </span>
        </Show>
        <Show when={meta().is_sidechain}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-sidechain">⤷ {t("session.subagent")}</span>
        </Show>
        <Show when={meta().project_path}>
          <span class="info-sep">&middot;</span>
          <span class="session-info-path">{meta().project_path}</span>
        </Show>
      </div>

      {/* Filter toolbar — only show roles that have messages */}
      <div class="filter-toolbar">
        <For each={(["user", "assistant", "tool", "system"] as MessageRole[]).filter((r) => (roleCounts()[r] || 0) > 0)}>
          {(role) => (
            <button
              class={`filter-btn${hiddenRoles().has(role) ? "" : " active"}`}
              onClick={() => toggleRole(role)}
            >
              {role === "user" ? t("session.filterUser") : role === "assistant" ? t("session.filterAssistant") : role === "tool" ? t("session.filterTool") : t("session.filterSystem")} ({roleCounts()[role]})
            </button>
          )}
        </For>
      </div>

      {/* In-session search bar */}
      <Show when={searchBarOpen()}>
        <div class="session-search-bar">
          <input
            class="session-search-input"
            type="text"
            placeholder={t("session.searchPlaceholder")}
            value={sessionSearch()}
            onInput={(e) => {
              setSessionSearch(e.currentTarget.value);
              setSearchMatchIdx(0);
              // Auto-jump to first match after DOM re-renders
              requestAnimationFrame(() => {
                const marks = getMarksInVisualOrder();
                if (marks.length > 0) {
                  messagesRef?.querySelector("mark.search-active")?.classList.remove("search-active");
                  marks[0].classList.add("search-active");
                  marks[0].scrollIntoView({ behavior: "smooth", block: "center" });
                }
              });
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") { e.shiftKey ? navigateSearchMatch(-1) : navigateSearchMatch(1); }
              if (e.key === "Escape") { setSearchBarOpen(false); setSessionSearch(""); }
            }}
          />
          <span class="session-search-count">
            {(() => {
              const total = getMarksInVisualOrder().length;
              if (total > 0) return `${searchMatchIdx() + 1}/${total}`;
              if (sessionSearch().trim()) return t("session.searchNoMatch");
              return "";
            })()}
          </span>
          <button class="session-search-nav" onClick={() => navigateSearchMatch(-1)} aria-label="Previous match">&uarr;</button>
          <button class="session-search-nav" onClick={() => navigateSearchMatch(1)} aria-label="Next match">&darr;</button>
          <button class="session-search-nav" onClick={() => { setSearchBarOpen(false); setSessionSearch(""); }} aria-label="Close search">&times;</button>
        </div>
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
        <div class="session-messages" ref={messagesRef} onScroll={handleMessagesScroll}>
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
                    <MergedToolRow tools={entry.tools} messages={entry.messages} highlightTerm={sessionSearch()} />
                  </div>
                );
              }
              return (
                <div class="session-entry" data-entry-key={entry.key}>
                  <MessageBubble message={entry.msg} provider={meta().provider} highlightTerm={sessionSearch()} />
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
        session={props.session}
        onClose={() => setShowExportDialog(false)}
      />
    </div>
  );
}
