import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock tauri plugins before importing the store
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(),
}));

// Reset module between tests so signals start fresh
describe("updater store", () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it("starts in idle phase", async () => {
    const { phase } = await import("./updater");
    expect(phase()).toBe("idle");
  });

  it("sets phase to available when update found", async () => {
    const { check } = await import("@tauri-apps/plugin-updater");
    vi.mocked(check).mockResolvedValue({
      version: "1.0.0",
      downloadAndInstall: vi.fn(),
    } as unknown as Awaited<ReturnType<typeof check>>);

    const { checkForUpdate, phase, availableVersion } =
      await import("./updater");
    await checkForUpdate();

    expect(phase()).toBe("available");
    expect(availableVersion()).toBe("1.0.0");
  });

  it("returns to idle when already up to date", async () => {
    const { check } = await import("@tauri-apps/plugin-updater");
    vi.mocked(check).mockResolvedValue(null);

    const { checkForUpdate, phase } = await import("./updater");
    await checkForUpdate();

    expect(phase()).toBe("idle");
  });

  it("sets error phase on check failure, then resets to idle", async () => {
    vi.useFakeTimers();
    const { check } = await import("@tauri-apps/plugin-updater");
    vi.mocked(check).mockRejectedValue(new Error("network error"));

    const { checkForUpdate, phase } = await import("./updater");
    await checkForUpdate();

    expect(phase()).toBe("error");
    vi.advanceTimersByTime(3000);
    expect(phase()).toBe("idle");
    vi.useRealTimers();
  });
});
