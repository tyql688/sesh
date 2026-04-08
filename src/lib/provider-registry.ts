import type { Provider } from "./types";

export interface ProviderWatchBehavior {
  debounceMs: number;
  matchPrefix: boolean;
}

/** Frontend-only provider behavior. Static provider facts live in Rust. */
interface ProviderUiConfig {
  displayLabel: (variantName?: string) => string;
  watch: ProviderWatchBehavior;
}

const REGISTRY: Record<Provider, ProviderUiConfig> = {
  claude: {
    displayLabel: () => "Claude Code",
    watch: { debounceMs: 300, matchPrefix: false },
  },
  codex: {
    displayLabel: () => "Codex",
    watch: { debounceMs: 300, matchPrefix: false },
  },
  gemini: {
    displayLabel: () => "Gemini",
    watch: { debounceMs: 2000, matchPrefix: true },
  },
  cursor: {
    displayLabel: () => "Cursor",
    watch: { debounceMs: 300, matchPrefix: false },
  },
  opencode: {
    displayLabel: () => "OpenCode",
    watch: { debounceMs: 2000, matchPrefix: false },
  },
  kimi: {
    displayLabel: () => "Kimi CLI",
    watch: { debounceMs: 300, matchPrefix: false },
  },
  "cc-mirror": {
    displayLabel: (variantName) => variantName ?? "CC-Mirror",
    watch: { debounceMs: 300, matchPrefix: false },
  },
  qwen: {
    displayLabel: () => "Qwen Code",
    watch: { debounceMs: 300, matchPrefix: false },
  },
};

/** Get the display label, using variant name for cc-mirror. */
export function getDisplayLabel(
  provider: Provider,
  variantName?: string,
): string {
  return REGISTRY[provider].displayLabel(variantName);
}

export function getProviderWatchBehavior(
  provider: Provider,
): ProviderWatchBehavior {
  return REGISTRY[provider].watch;
}
