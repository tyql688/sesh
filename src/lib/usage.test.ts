import { describe, expect, it } from "vitest";

import {
  buildDailyChartData,
  buildHoveredDaySummary,
  compareUsageValues,
  makeEmptyUsageStats,
  totalUsageTokens,
} from "./usage";

describe("makeEmptyUsageStats", () => {
  it("returns zeroed usage collections", () => {
    expect(makeEmptyUsageStats()).toEqual({
      total_sessions: 0,
      total_turns: 0,
      total_input_tokens: 0,
      total_output_tokens: 0,
      total_cache_read_tokens: 0,
      total_cache_write_tokens: 0,
      total_cost: 0,
      cache_hit_rate: 0,
      daily_usage: [],
      model_costs: [],
      project_costs: [],
      recent_sessions: [],
    });
  });
});

describe("compareUsageValues", () => {
  it("sorts strings respecting direction", () => {
    expect(compareUsageValues("a", "b", true)).toBeLessThan(0);
    expect(compareUsageValues("a", "b", false)).toBeGreaterThan(0);
  });

  it("falls back to numeric comparison", () => {
    expect(compareUsageValues(1, 2, true)).toBeLessThan(0);
    expect(compareUsageValues(1, 2, false)).toBeGreaterThan(0);
  });
});

describe("totalUsageTokens", () => {
  it("sums all token categories", () => {
    expect(
      totalUsageTokens({
        ...makeEmptyUsageStats(),
        total_input_tokens: 10,
        total_output_tokens: 20,
        total_cache_read_tokens: 30,
        total_cache_write_tokens: 40,
      }),
    ).toBe(100);
  });
});

describe("buildDailyChartData", () => {
  it("groups dates and filters providers without activity", () => {
    const chartData = buildDailyChartData(
      [
        { date: "2026-04-09", provider: "claude", tokens: 40 },
        { date: "2026-04-09", provider: "codex", tokens: 10 },
        { date: "2026-04-10", provider: "claude", tokens: 15 },
      ],
      ["claude", "codex", "kimi"],
    );

    expect(chartData.dates).toEqual(["2026-04-09", "2026-04-10"]);
    expect(chartData.providers).toEqual(["claude", "codex"]);
    expect(chartData.maxTokens).toBe(50);
    expect(chartData.byDate.get("2026-04-10")?.get("claude")).toBe(15);
  });
});

describe("buildHoveredDaySummary", () => {
  it("builds a sorted breakdown for the selected date", () => {
    const chartData = buildDailyChartData(
      [
        { date: "2026-04-09", provider: "claude", tokens: 40 },
        { date: "2026-04-09", provider: "codex", tokens: 10 },
      ],
      ["claude", "codex"],
    );

    const summary = buildHoveredDaySummary(
      "2026-04-09",
      chartData,
      (provider) => ({
        label: provider.toUpperCase(),
        color: `var(--${provider})`,
      }),
    );

    expect(summary).toMatchObject({
      date: "2026-04-09",
      total: 50,
      breakdown: [
        {
          provider: "claude",
          label: "CLAUDE",
          color: "var(--claude)",
          tokens: 40,
        },
        {
          provider: "codex",
          label: "CODEX",
          color: "var(--codex)",
          tokens: 10,
        },
      ],
    });
    expect(summary?.xPercent).toBe(50);
  });

  it("returns null when the date is missing", () => {
    const chartData = buildDailyChartData([], []);
    expect(
      buildHoveredDaySummary(null, chartData, () => ({ label: "", color: "" })),
    ).toBeNull();
    expect(
      buildHoveredDaySummary("2026-04-09", chartData, () => ({
        label: "",
        color: "",
      })),
    ).toBeNull();
  });
});
