import type { Provider } from "./types";

/** Per-provider configuration. Adding a provider = adding one entry here. */
export interface ProviderDef {
  key: Provider;
  label: string;
  /** CSS variable name (without --), e.g. "claude" -> var(--claude) */
  colorVar: string;
  /** Display label, optionally using variant name */
  displayLabel: (variantName?: string) => string;
  /** How the frontend watches for live changes */
  watchStrategy: "fs" | "poll";
  /** Debounce delay in ms for watching */
  watchDebounceMs: number;
  /** Whether FS change matching should use directory prefix (not exact path) */
  watchMatchPrefix: boolean;
  /** Sort order in the sidebar tree */
  sortOrder: number;
}

const REGISTRY: Record<Provider, ProviderDef> = {
  claude: {
    key: "claude",
    label: "Claude Code",
    colorVar: "claude",
    displayLabel: () => "Claude Code",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 0,
  },
  codex: {
    key: "codex",
    label: "Codex",
    colorVar: "codex",
    displayLabel: () => "Codex",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 2,
  },
  gemini: {
    key: "gemini",
    label: "Gemini",
    colorVar: "gemini",
    displayLabel: () => "Gemini",
    watchStrategy: "poll",
    watchDebounceMs: 2000,
    watchMatchPrefix: true,
    sortOrder: 3,
  },
  cursor: {
    key: "cursor",
    label: "Cursor",
    colorVar: "cursor",
    displayLabel: () => "Cursor",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 4,
  },
  opencode: {
    key: "opencode",
    label: "OpenCode",
    colorVar: "opencode",
    displayLabel: () => "OpenCode",
    watchStrategy: "poll",
    watchDebounceMs: 2000,
    watchMatchPrefix: false,
    sortOrder: 5,
  },
  kimi: {
    key: "kimi",
    label: "Kimi CLI",
    colorVar: "kimi",
    displayLabel: () => "Kimi CLI",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 6,
  },
  "cc-mirror": {
    key: "cc-mirror",
    label: "CC-Mirror",
    colorVar: "cc-mirror",
    displayLabel: (variantName) => variantName ?? "CC-Mirror",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 1,
  },
  qwen: {
    key: "qwen",
    label: "Qwen Code",
    colorVar: "qwen",
    displayLabel: () => "Qwen Code",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 7,
  },
};

export function getProvider(provider: Provider): ProviderDef {
  return REGISTRY[provider];
}

export function getProviderLabel(provider: Provider): string {
  return REGISTRY[provider]?.displayLabel() ?? provider;
}

export function getProviderColor(provider: Provider): string {
  return `var(--${REGISTRY[provider]?.colorVar ?? provider})`;
}

export function allProviders(): ProviderDef[] {
  return Object.values(REGISTRY);
}

/** Get the display label, using variant name for cc-mirror. */
export function getDisplayLabel(
  provider: Provider,
  variantName?: string,
): string {
  return REGISTRY[provider].displayLabel(variantName);
}
