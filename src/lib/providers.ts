import type { Provider } from "./types";

export interface ProviderConfig {
  key: Provider;
  label: string;
  color: string; // CSS variable name (without --)
  resumePrefix: string; // e.g. "claude --resume"
}

const PROVIDERS: Record<Provider, ProviderConfig> = {
  claude: { key: "claude", label: "Claude Code", color: "claude", resumePrefix: "claude --resume" },
  codex: { key: "codex", label: "Codex", color: "codex", resumePrefix: "codex resume" },
  gemini: { key: "gemini", label: "Gemini", color: "gemini", resumePrefix: "gemini --resume" },
  cursor: { key: "cursor", label: "Cursor", color: "cursor", resumePrefix: "cursor --resume" },
  opencode: { key: "opencode", label: "OpenCode", color: "opencode", resumePrefix: "opencode --resume" },
  kimi: { key: "kimi", label: "Kimi CLI", color: "kimi", resumePrefix: "kimi --session" },
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
