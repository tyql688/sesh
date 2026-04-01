import type { Provider } from "./types";

/** Per-provider configuration. Adding a provider = adding one entry here. */
export interface ProviderDef {
  key: Provider;
  label: string;
  /** CSS variable name (without --), e.g. "claude" -> var(--claude) */
  colorVar: string;
  /** Build the CLI resume command */
  resumeCommand: (id: string) => string;
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
    resumeCommand: (id) => `claude --resume ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 0,
  },
  codex: {
    key: "codex",
    label: "Codex",
    colorVar: "codex",
    resumeCommand: (id) => `codex resume ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 2,
  },
  gemini: {
    key: "gemini",
    label: "Gemini",
    colorVar: "gemini",
    resumeCommand: (id) => `gemini --resume ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 800,
    watchMatchPrefix: true,
    sortOrder: 3,
  },
  cursor: {
    key: "cursor",
    label: "Cursor",
    colorVar: "cursor",
    resumeCommand: (id) => `agent --resume=${id}`,
    watchStrategy: "poll",
    watchDebounceMs: 2000,
    watchMatchPrefix: false,
    sortOrder: 4,
  },
  opencode: {
    key: "opencode",
    label: "OpenCode",
    colorVar: "opencode",
    resumeCommand: (id) => `opencode -s ${id}`,
    watchStrategy: "poll",
    watchDebounceMs: 2000,
    watchMatchPrefix: false,
    sortOrder: 5,
  },
  kimi: {
    key: "kimi",
    label: "Kimi CLI",
    colorVar: "kimi",
    resumeCommand: (id) => `kimi --session ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 6,
  },
  "cc-mirror": {
    key: "cc-mirror",
    label: "CC-Mirror",
    colorVar: "cc-mirror",
    resumeCommand: () => "",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 1,
  },
};

export function getProvider(provider: Provider): ProviderDef {
  return REGISTRY[provider];
}

export function getProviderLabel(provider: Provider): string {
  return REGISTRY[provider]?.label ?? provider;
}

export function getProviderColor(provider: Provider): string {
  return `var(--${REGISTRY[provider]?.colorVar ?? provider})`;
}

export function allProviders(): ProviderDef[] {
  return Object.values(REGISTRY);
}

/** Build resume command, handling cc-mirror variant names. */
export function buildResumeCommand(
  provider: Provider,
  sessionId: string,
  variantName?: string,
): string {
  if (provider === "cc-mirror" && variantName) {
    return `${variantName} --resume ${sessionId}`;
  }
  return REGISTRY[provider].resumeCommand(sessionId);
}

/** Get the display label, using variant name for cc-mirror. */
export function getDisplayLabel(
  provider: Provider,
  variantName?: string,
): string {
  if (provider === "cc-mirror" && variantName) {
    return variantName;
  }
  return REGISTRY[provider].label;
}
