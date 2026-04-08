import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderCatalogItem } from "../lib/types";

const getProviderCatalog = vi.fn<() => Promise<ProviderCatalogItem[]>>();

vi.mock("../lib/tauri", () => ({
  getProviderCatalog,
}));

async function loadStore() {
  return import("./providerCatalog");
}

describe("providerCatalog store", () => {
  beforeEach(() => {
    vi.resetModules();
    getProviderCatalog.mockReset();
  });

  it("uses fallback values before catalog loads", async () => {
    const {
      getProviderLabel,
      getProvidersForWatchStrategy,
      getProviderSortOrder,
      getProviderWatchStrategy,
    } = await loadStore();

    expect(getProviderLabel("claude")).toBe("Claude Code");
    expect(getProviderLabel("cc-mirror", "cczai")).toBe("cczai");
    expect(getProviderLabel("cc-mirror")).toBe("CC-Mirror");
    expect(getProviderWatchStrategy("gemini")).toBe("poll");
    expect(getProvidersForWatchStrategy("poll")).toEqual([
      "gemini",
      "opencode",
    ]);
    expect(getProviderSortOrder("claude")).toBeLessThan(
      getProviderSortOrder("codex"),
    );
  });

  it("switches watch providers to the loaded catalog", async () => {
    getProviderCatalog.mockResolvedValue([
      {
        key: "claude",
        label: "Claude Code",
        color: "var(--claude)",
        sort_order: 0,
        watch_strategy: "fs",
      },
      {
        key: "codex",
        label: "Codex",
        color: "var(--codex)",
        sort_order: 1,
        watch_strategy: "poll",
      },
    ]);

    const {
      getProvidersForWatchStrategy,
      getProviderCatalogVersion,
      loadProviderCatalog,
    } = await loadStore();

    expect(getProvidersForWatchStrategy("poll")).toEqual([
      "gemini",
      "opencode",
    ]);

    await loadProviderCatalog();

    expect(getProviderCatalogVersion()).toBe(1);
    expect(getProvidersForWatchStrategy("poll")).toEqual(["codex"]);
  });

  it("keeps fallback values and warns when catalog load fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    getProviderCatalog.mockRejectedValue(new Error("boom"));

    const {
      getProvidersForWatchStrategy,
      getProviderCatalogVersion,
      loadProviderCatalog,
    } = await loadStore();

    await loadProviderCatalog();

    expect(getProviderCatalogVersion()).toBe(0);
    expect(getProvidersForWatchStrategy("poll")).toEqual([
      "gemini",
      "opencode",
    ]);
    expect(warn).toHaveBeenCalledWith(
      "failed to load provider catalog:",
      expect.any(Error),
    );

    warn.mockRestore();
  });
});
