import { createSignal } from "solid-js";
import { searchSessions } from "../lib/tauri";
import type { SearchResult, SearchFilters } from "../lib/types";

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchResult[]>([]);
const [isSearching, setIsSearching] = createSignal(false);

let debounceTimer: ReturnType<typeof setTimeout>;

function parseSearchQuery(raw: string): SearchFilters {
  let remaining = raw;
  let provider: string | undefined;
  let project: string | undefined;
  let after: number | undefined;
  let before: number | undefined;

  const extract = (prefix: string): string | undefined => {
    const regex = new RegExp(`${prefix}:(\\S+)`, "i");
    const match = remaining.match(regex);
    if (match) {
      remaining = remaining.replace(match[0], "").trim();
      return match[1];
    }
    return undefined;
  };

  provider = extract("provider");
  project = extract("project");

  const afterStr = extract("after");
  if (afterStr) {
    const d = Date.parse(afterStr);
    if (!isNaN(d)) {
      after = Math.floor(d / 1000);
    }
  }

  const beforeStr = extract("before");
  if (beforeStr) {
    const d = Date.parse(beforeStr);
    if (!isNaN(d)) {
      before = Math.floor(d / 1000);
    }
  }

  return {
    query: remaining.trim(),
    provider,
    project,
    after,
    before,
  };
}

function search(q: string) {
  setQuery(q);
  clearTimeout(debounceTimer);
  if (!q.trim()) {
    setResults([]);
    setIsSearching(false);
    return;
  }
  setIsSearching(true);
  debounceTimer = setTimeout(async () => {
    try {
      const filters = parseSearchQuery(q);
      const r = await searchSessions(filters);
      setResults(r);
    } catch (e) {
      console.warn("search failed:", e);
      setResults([]);
    } finally {
      setIsSearching(false);
    }
  }, 150);
}

function clearSearch() {
  setQuery("");
  setResults([]);
  setIsSearching(false);
  clearTimeout(debounceTimer);
}

export { query, setQuery, results, isSearching, search, clearSearch, parseSearchQuery };
