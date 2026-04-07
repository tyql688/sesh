import { describe, expect, it } from "vitest";
import {
  getProvider,
  getProviderLabel,
  getProviderColor,
  getDisplayLabel,
  allProviders,
} from "./provider-registry";
import type { Provider } from "./types";

const ALL_PROVIDERS: Provider[] = [
  "claude",
  "codex",
  "gemini",
  "cursor",
  "opencode",
  "kimi",
  "cc-mirror",
  "qwen",
];

describe("provider-registry", () => {
  it("getProvider returns config for all providers", () => {
    for (const key of ALL_PROVIDERS) {
      const def = getProvider(key);
      expect(def).toBeDefined();
      expect(def.key).toBe(key);
      expect(def.label).toBeTruthy();
      expect(def.colorVar).toBeTruthy();
    }
  });

  it("allProviders returns all entries", () => {
    expect(allProviders()).toHaveLength(ALL_PROVIDERS.length);
  });

  it("getProviderLabel returns label", () => {
    expect(getProviderLabel("claude")).toBe("Claude Code");
    expect(getProviderLabel("kimi")).toBe("Kimi CLI");
  });

  it("getProviderColor returns CSS var", () => {
    expect(getProviderColor("claude")).toBe("var(--claude)");
  });

  it("getDisplayLabel uses variant for cc-mirror", () => {
    expect(getDisplayLabel("claude")).toBe("Claude Code");
    expect(getDisplayLabel("cc-mirror", "cczai")).toBe("cczai");
    expect(getDisplayLabel("cc-mirror")).toBe("CC-Mirror");
  });

  it("each provider has unique sortOrder", () => {
    const orders = allProviders().map((p) => p.sortOrder);
    const unique = new Set(orders);
    expect(unique.size).toBe(orders.length);
  });
});
