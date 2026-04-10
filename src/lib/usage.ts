import type { DailyUsage, UsageStats } from "./types";

export type UsageSortState = { col: string; asc: boolean };

export interface UsageChartProviderMeta {
  label: string;
  color: string;
}

export interface UsageDailyChartData {
  dates: string[];
  byDate: Map<string, Map<string, number>>;
  providers: string[];
  maxTokens: number;
}

export interface HoveredDaySummary {
  date: string;
  total: number;
  xPercent: number;
  breakdown: Array<{
    provider: string;
    label: string;
    color: string;
    tokens: number;
  }>;
}

export const ROW_LIMIT_OPTIONS = [10, 25, 50, 100] as const;

export function makeEmptyUsageStats(): UsageStats {
  return {
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
  };
}

export function compareUsageValues(
  left: unknown,
  right: unknown,
  asc: boolean,
): number {
  if (typeof left === "string" && typeof right === "string") {
    return asc ? left.localeCompare(right) : right.localeCompare(left);
  }

  const leftNumber = typeof left === "number" ? left : 0;
  const rightNumber = typeof right === "number" ? right : 0;
  return asc ? leftNumber - rightNumber : rightNumber - leftNumber;
}

export function totalUsageTokens(data: UsageStats | undefined): number {
  if (!data) return 0;
  return (
    data.total_input_tokens +
    data.total_output_tokens +
    data.total_cache_read_tokens +
    data.total_cache_write_tokens
  );
}

export function buildDailyChartData(
  dailyUsage: DailyUsage[],
  selectedProviderKeys: string[],
): UsageDailyChartData {
  const byDate = new Map<string, Map<string, number>>();
  let maxTokens = 0;

  for (const item of dailyUsage) {
    if (!byDate.has(item.date)) byDate.set(item.date, new Map());
    byDate.get(item.date)!.set(item.provider, item.tokens);
  }

  for (const providerMap of byDate.values()) {
    const total = [...providerMap.values()].reduce((sum, value) => sum + value, 0);
    if (total > maxTokens) maxTokens = total;
  }

  const providers = selectedProviderKeys.filter((key) =>
    [...byDate.values()].some((providerMap) => (providerMap.get(key) ?? 0) > 0),
  );

  return {
    dates: [...byDate.keys()].sort(),
    byDate,
    providers,
    maxTokens: maxTokens || 1,
  };
}

export function buildHoveredDaySummary(
  date: string | null,
  chartData: UsageDailyChartData,
  getProviderMeta: (provider: string) => UsageChartProviderMeta,
): HoveredDaySummary | null {
  if (!date) return null;

  const dateIndex = chartData.dates.indexOf(date);
  if (dateIndex === -1) return null;

  const providerMap = chartData.byDate.get(date);
  if (!providerMap) return null;

  const breakdown = chartData.providers
    .map((provider) => {
      const tokens = providerMap.get(provider) ?? 0;
      const meta = getProviderMeta(provider);
      return {
        provider,
        label: meta.label,
        color: meta.color,
        tokens,
      };
    })
    .filter((entry) => entry.tokens > 0)
    .sort((left, right) => right.tokens - left.tokens);

  return {
    date,
    total: breakdown.reduce((sum, entry) => sum + entry.tokens, 0),
    xPercent: ((dateIndex + 0.5) / chartData.dates.length) * 100,
    breakdown,
  };
}
