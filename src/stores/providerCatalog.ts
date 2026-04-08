import { createSignal } from "solid-js";
import { getProviderCatalog } from "../lib/tauri";
import type { Provider, ProviderCatalogItem } from "../lib/types";

type ProviderCatalogMap = Partial<Record<Provider, ProviderCatalogItem>>;
type ProviderWatchStrategy = ProviderCatalogItem["watch_strategy"];

const [providerCatalog, setProviderCatalog] = createSignal<ProviderCatalogMap>(
  {},
);
const [providerCatalogVersion, setProviderCatalogVersion] = createSignal(0);

const FALLBACK_PROVIDER_CATALOG: Record<Provider, ProviderCatalogItem> = {
  claude: {
    key: "claude",
    label: "Claude Code",
    color: "var(--claude)",
    sort_order: 0,
    watch_strategy: "fs",
  },
  "cc-mirror": {
    key: "cc-mirror",
    label: "CC-Mirror",
    color: "var(--cc-mirror)",
    sort_order: 1,
    watch_strategy: "fs",
  },
  codex: {
    key: "codex",
    label: "Codex",
    color: "var(--codex)",
    sort_order: 2,
    watch_strategy: "fs",
  },
  gemini: {
    key: "gemini",
    label: "Gemini",
    color: "var(--gemini)",
    sort_order: 3,
    watch_strategy: "poll",
  },
  cursor: {
    key: "cursor",
    label: "Cursor",
    color: "var(--cursor)",
    sort_order: 4,
    watch_strategy: "fs",
  },
  opencode: {
    key: "opencode",
    label: "OpenCode",
    color: "var(--opencode)",
    sort_order: 5,
    watch_strategy: "poll",
  },
  kimi: {
    key: "kimi",
    label: "Kimi CLI",
    color: "var(--kimi)",
    sort_order: 6,
    watch_strategy: "fs",
  },
  qwen: {
    key: "qwen",
    label: "Qwen Code",
    color: "var(--qwen)",
    sort_order: 7,
    watch_strategy: "fs",
  },
};

let loadPromise: Promise<void> | null = null;

export async function loadProviderCatalog() {
  if (Object.keys(providerCatalog()).length > 0) {
    return;
  }

  if (loadPromise) return loadPromise;

  loadPromise = getProviderCatalog()
    .then((items) => {
      const next: ProviderCatalogMap = {};
      for (const item of items) {
        next[item.key] = item;
      }
      setProviderCatalog(next);
      setProviderCatalogVersion((version) => version + 1);
    })
    .catch((error) => {
      console.warn("failed to load provider catalog:", error);
    })
    .finally(() => {
      loadPromise = null;
    });

  return loadPromise;
}

function activeProviderCatalog(): ProviderCatalogMap {
  const loaded = providerCatalog();
  return Object.keys(loaded).length > 0 ? loaded : FALLBACK_PROVIDER_CATALOG;
}

export function getProviderCatalogItem(provider: Provider) {
  return providerCatalog()[provider];
}

export function getProviderLabel(
  provider: Provider,
  variantName?: string,
): string {
  if (provider === "cc-mirror" && variantName) {
    return variantName;
  }

  return (
    getProviderCatalogItem(provider)?.label ??
    FALLBACK_PROVIDER_CATALOG[provider].label
  );
}

export function getProviderColor(provider: Provider): string {
  return (
    getProviderCatalogItem(provider)?.color ??
    FALLBACK_PROVIDER_CATALOG[provider].color
  );
}

export function getProviderWatchStrategy(
  provider: Provider,
): ProviderWatchStrategy {
  return (
    getProviderCatalogItem(provider)?.watch_strategy ??
    FALLBACK_PROVIDER_CATALOG[provider].watch_strategy
  );
}

export function getProvidersForWatchStrategy(
  strategy: ProviderWatchStrategy,
): Provider[] {
  const catalog = activeProviderCatalog();
  return (Object.entries(catalog) as [Provider, ProviderCatalogItem][])
    .filter(([, item]) => item.watch_strategy === strategy)
    .map(([provider]) => provider);
}

export function getProviderSortOrder(provider: Provider): number {
  return (
    getProviderCatalogItem(provider)?.sort_order ??
    FALLBACK_PROVIDER_CATALOG[provider].sort_order
  );
}

export function getProviderCatalogVersion(): number {
  return providerCatalogVersion();
}
