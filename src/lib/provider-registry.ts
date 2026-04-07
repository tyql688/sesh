import type { Provider } from "./types";

export interface ProviderWatchConfig {
  strategy: "fs" | "poll";
  debounceMs: number;
  matchPrefix: boolean;
}

/** Frontend-only provider behavior. Static provider facts live in Rust. */
interface ProviderUiConfig {
  displayLabel: (variantName?: string) => string;
  watch: ProviderWatchConfig;
}

const REGISTRY: Record<Provider, ProviderUiConfig> = {
  claude: {
    displayLabel: () => "Claude Code",
    watch: { strategy: "fs", debounceMs: 300, matchPrefix: false },
  },
  codex: {
    displayLabel: () => "Codex",
    watch: { strategy: "fs", debounceMs: 300, matchPrefix: false },
  },
  gemini: {
    displayLabel: () => "Gemini",
    watch: { strategy: "poll", debounceMs: 2000, matchPrefix: true },
  },
  cursor: {
    displayLabel: () => "Cursor",
    watch: { strategy: "fs", debounceMs: 300, matchPrefix: false },
  },
  opencode: {
    displayLabel: () => "OpenCode",
    watch: { strategy: "poll", debounceMs: 2000, matchPrefix: false },
  },
  kimi: {
    displayLabel: () => "Kimi CLI",
    watch: { strategy: "fs", debounceMs: 300, matchPrefix: false },
  },
  "cc-mirror": {
    displayLabel: (variantName) => variantName ?? "CC-Mirror",
    watch: { strategy: "fs", debounceMs: 300, matchPrefix: false },
  },
  qwen: {
    displayLabel: () => "Qwen Code",
    watch: { strategy: "fs", debounceMs: 300, matchPrefix: false },
  },
};

/** Get the display label, using variant name for cc-mirror. */
export function getDisplayLabel(
  provider: Provider,
  variantName?: string,
): string {
  return REGISTRY[provider].displayLabel(variantName);
}

export function getProviderWatchConfig(
  provider: Provider,
): ProviderWatchConfig {
  return REGISTRY[provider].watch;
}

export function providersForWatchStrategy(
  strategy: ProviderWatchConfig["strategy"],
): Provider[] {
  return (Object.keys(REGISTRY) as Provider[]).filter(
    (provider) => REGISTRY[provider].watch.strategy === strategy,
  );
}
