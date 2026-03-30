import type { Provider } from "./types";

export interface ProviderConfig {
  key: Provider;
  label: string;
  color: string; // CSS variable name (without --)
  resumeCommand: (id: string) => string;
}

const PROVIDERS: Record<Provider, ProviderConfig> = {
  claude: {
    key: "claude",
    label: "Claude Code",
    color: "claude",
    resumeCommand: (id) => `claude --resume ${id}`,
  },
  codex: {
    key: "codex",
    label: "Codex",
    color: "codex",
    resumeCommand: (id) => `codex resume ${id}`,
  },
  gemini: {
    key: "gemini",
    label: "Gemini",
    color: "gemini",
    resumeCommand: (id) => `gemini --resume ${id}`,
  },
  cursor: {
    key: "cursor",
    label: "Cursor",
    color: "cursor",
    resumeCommand: (id) => `agent --resume=${id}`,
  },
  opencode: {
    key: "opencode",
    label: "OpenCode",
    color: "opencode",
    resumeCommand: (id) => `opencode -s ${id}`,
  },
  kimi: {
    key: "kimi",
    label: "Kimi CLI",
    color: "kimi",
    resumeCommand: (id) => `kimi --session ${id}`,
  },
  "cc-mirror": {
    key: "cc-mirror",
    label: "CC-Mirror",
    color: "cc-mirror",
    // cc-mirror uses variant_name as command; handled separately in Explorer
    resumeCommand: () => "",
  },
};

export function getProviderLabel(provider: Provider): string {
  return PROVIDERS[provider]?.label ?? provider;
}

export function getProviderColor(provider: Provider): string {
  return `var(--${PROVIDERS[provider]?.color ?? provider})`;
}

export function getProviderConfig(provider: Provider): ProviderConfig {
  return PROVIDERS[provider];
}

export function allProviders(): ProviderConfig[] {
  return Object.values(PROVIDERS);
}
