import type { Accessor, Setter } from "solid-js";
import { useI18n } from "../../i18n/index";

export function SessionSearch(props: {
  sessionSearch: Accessor<string>;
  setSessionSearch: Setter<string>;
  searchMatchIdx: Accessor<number>;
  setSearchMatchIdx: Setter<number>;
  setSearchBarOpen: Setter<boolean>;
  messagesRef: HTMLDivElement | undefined;
}) {
  const { t } = useI18n();

  /** Get marks in visual order (top->bottom). Sort by position since column-reverse
   *  flips message order but not text order within each message. */
  function getMarksInVisualOrder(): Element[] {
    if (!props.messagesRef) return [];
    const marks = Array.from(props.messagesRef.querySelectorAll("mark.search-highlight"));
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
    props.messagesRef?.querySelector("mark.search-active")?.classList.remove("search-active");
    const newIdx = (props.searchMatchIdx() + delta + marks.length) % marks.length;
    props.setSearchMatchIdx(newIdx);
    const target = marks[newIdx];
    target.classList.add("search-active");
    target.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  return (
    <div class="session-search-bar">
      <input
        class="session-search-input"
        type="text"
        placeholder={t("session.searchPlaceholder")}
        value={props.sessionSearch()}
        onInput={(e) => {
          props.setSessionSearch(e.currentTarget.value);
          props.setSearchMatchIdx(0);
          // Auto-jump to first match after DOM re-renders
          requestAnimationFrame(() => {
            const marks = getMarksInVisualOrder();
            if (marks.length > 0) {
              props.messagesRef?.querySelector("mark.search-active")?.classList.remove("search-active");
              marks[0].classList.add("search-active");
              marks[0].scrollIntoView({ behavior: "smooth", block: "center" });
            }
          });
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            if (e.shiftKey) {
              navigateSearchMatch(-1);
            } else {
              navigateSearchMatch(1);
            }
          }
          if (e.key === "Escape") {
            props.setSearchBarOpen(false);
            props.setSessionSearch("");
          }
        }}
      />
      <span class="session-search-count">
        {(() => {
          const total = getMarksInVisualOrder().length;
          if (total > 0) return `${props.searchMatchIdx() + 1}/${total}`;
          if (props.sessionSearch().trim()) return t("session.searchNoMatch");
          return "";
        })()}
      </span>
      <button class="session-search-nav" onClick={() => navigateSearchMatch(-1)} aria-label="Previous match">
        &uarr;
      </button>
      <button class="session-search-nav" onClick={() => navigateSearchMatch(1)} aria-label="Next match">
        &darr;
      </button>
      <button
        class="session-search-nav"
        onClick={() => {
          props.setSearchBarOpen(false);
          props.setSessionSearch("");
        }}
        aria-label="Close search"
      >
        &times;
      </button>
    </div>
  );
}
