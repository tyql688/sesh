import { createSignal, Show, For, onCleanup } from "solid-js";
import type { SessionMeta } from "../lib/types";
import {
  query,
  results,
  isSearching,
  search,
  clearSearch,
} from "../stores/search";
import { useI18n } from "../i18n/index";
import { ProviderIcon } from "../lib/icons";
import { isMac } from "../lib/platform";

function sanitizeSnippet(html: string): string {
  // Escape all HTML first, then restore only <mark> and </mark> from FTS5 snippet
  const escaped = html
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
  return escaped
    .replace(/&lt;mark&gt;/gi, "<mark>")
    .replace(/&lt;\/mark&gt;/gi, "</mark>");
}

export function SearchPanel(props: {
  onOpenSession: (session: SessionMeta) => void;
}) {
  const { t } = useI18n();
  const [focused, setFocused] = createSignal(false);
  const [selectedIndex, setSelectedIndex] = createSignal(-1);
  let inputRef: HTMLInputElement | undefined;

  let blurTimer: ReturnType<typeof setTimeout> | undefined;

  function handleInput(e: InputEvent) {
    const target = e.currentTarget as HTMLInputElement;
    search(target.value);
    setSelectedIndex(-1);
  }

  function handleFocus() {
    setFocused(true);
  }

  function handleBlur() {
    clearTimeout(blurTimer);
    blurTimer = setTimeout(() => {
      setFocused(false);
      setSelectedIndex(-1);
    }, 200);
  }

  function handleResultClick(session: SessionMeta) {
    props.onOpenSession(session);
    clearSearch();
    setFocused(false);
    setSelectedIndex(-1);
    inputRef?.blur();
  }

  function handleKeyDown(e: KeyboardEvent) {
    const r = results();

    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((i) => (i < r.length - 1 ? i + 1 : 0));
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((i) => (i > 0 ? i - 1 : r.length - 1));
      return;
    }

    if (e.key === "Enter") {
      e.preventDefault();
      const idx = selectedIndex();
      if (idx >= 0 && idx < r.length) {
        handleResultClick(r[idx].session);
      }
      return;
    }

    if (e.key === "Escape") {
      clearSearch();
      setFocused(false);
      setSelectedIndex(-1);
      inputRef?.blur();
    }
  }

  function focusInput() {
    inputRef?.focus();
  }

  onCleanup(() => {
    clearTimeout(blurTimer);
  });

  return (
    <div
      class="search-panel"
      data-focus-search
      ref={(el) => {
        (el as HTMLElement & { __focusInput?: () => void }).__focusInput =
          focusInput;
      }}
    >
      <div class="search-input-wrapper">
        <svg
          class="search-icon"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <circle cx="11" cy="11" r="8" />
          <path d="M21 21l-4.35-4.35" />
        </svg>
        <input
          ref={inputRef}
          class="search-input"
          type="text"
          aria-label={t("search.ariaLabel")}
          placeholder={t("search.placeholder")}
          value={query()}
          onInput={handleInput}
          onFocus={handleFocus}
          onBlur={handleBlur}
          onKeyDown={handleKeyDown}
        />
        <kbd class="search-shortcut">
          {isMac ? "\u21E7\u2318" : "Ctrl+Shift+"}F
        </kbd>
      </div>
      <Show when={focused() && query().trim().length > 0}>
        <div class="search-dropdown">
          <Show when={isSearching()}>
            <div class="search-loading">
              <div class="spinner spinner-sm" />
            </div>
          </Show>
          <Show
            when={
              !isSearching() &&
              results().length === 0 &&
              query().trim().length > 0
            }
          >
            <div class="search-no-results">{t("search.noResults")}</div>
          </Show>
          <For each={results()}>
            {(result, i) => (
              <button
                class="search-result-item"
                classList={{ selected: selectedIndex() === i() }}
                onMouseDown={() => handleResultClick(result.session)}
                onMouseEnter={() => setSelectedIndex(i())}
              >
                <span
                  class="provider-dot provider-logo"
                  style={{ color: `var(--${result.session.provider})` }}
                >
                  <ProviderIcon provider={result.session.provider} />
                </span>
                <div class="search-result-text">
                  <span class="search-result-title">
                    {result.session.title}
                  </span>
                  <span
                    class="search-result-snippet"
                    // eslint-disable-next-line solid/no-innerhtml -- sanitizeSnippet escapes then restores <mark> only
                    innerHTML={sanitizeSnippet(result.snippet)}
                  />
                </div>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
