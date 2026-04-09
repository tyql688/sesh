import {
  createSignal,
  createResource,
  createMemo,
  createEffect,
  For,
  Show,
} from "solid-js";
import { useI18n } from "../i18n/index";
import { getUsageStats, getSessionCount } from "../lib/tauri";
import { listProviderSnapshots } from "../stores/providerSnapshots";
import type { ModelCost, ProjectCost, SessionCostRow } from "../lib/types";

type SortState = { col: string; asc: boolean };

export function UsagePanel() {
  const { t } = useI18n();

  // State
  const [rangeDays, setRangeDays] = createSignal<number | null>(7);
  const [selectedProviders, setSelectedProviders] = createSignal<Set<string>>(
    new Set(),
  );

  // Initialize providers from snapshots — select all that exist
  createEffect(() => {
    const snapshots = listProviderSnapshots();
    if (snapshots.length > 0 && selectedProviders().size === 0) {
      setSelectedProviders(
        new Set(snapshots.filter((s) => s.exists).map((s) => s.key)),
      );
    }
  });

  // Toggle provider
  const toggleProvider = (key: string) => {
    setSelectedProviders((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  // Fetch data reactively
  const [stats] = createResource(
    () => ({ providers: [...selectedProviders()], range: rangeDays() }),
    (params) => getUsageStats(params.providers, params.range),
  );

  // For empty state detection
  const [sessionCount] = createResource(() => getSessionCount());

  // Sort state for each table
  const [modelSort, setModelSort] = createSignal<SortState>({
    col: "cost",
    asc: false,
  });
  const [projectSort, setProjectSort] = createSignal<SortState>({
    col: "cost",
    asc: false,
  });
  const [sessionSort, setSessionSort] = createSignal<SortState>({
    col: "updated_at",
    asc: false,
  });

  // Sort toggle helper
  const toggleSort = (
    setter: (fn: (prev: SortState) => SortState) => void,
    col: string,
  ) => {
    setter((prev: SortState) => ({
      col,
      asc: prev.col === col ? !prev.asc : false,
    }));
  };

  // Generic comparator
  const compareValues = (va: unknown, vb: unknown, asc: boolean): number => {
    if (typeof va === "string" && typeof vb === "string") {
      return asc ? va.localeCompare(vb) : vb.localeCompare(va);
    }
    const na = typeof va === "number" ? va : 0;
    const nb = typeof vb === "number" ? vb : 0;
    return asc ? na - nb : nb - na;
  };

  // Sorted data memos
  const sortedModels = createMemo(() => {
    const data = stats()?.model_costs ?? [];
    const { col, asc } = modelSort();
    return [...data].sort((a, b) =>
      compareValues(a[col as keyof ModelCost], b[col as keyof ModelCost], asc),
    );
  });

  const sortedProjects = createMemo(() => {
    const data = stats()?.project_costs ?? [];
    const { col, asc } = projectSort();
    return [...data].sort((a, b) =>
      compareValues(
        a[col as keyof ProjectCost],
        b[col as keyof ProjectCost],
        asc,
      ),
    );
  });

  const sortedSessions = createMemo(() => {
    const data = stats()?.recent_sessions ?? [];
    const { col, asc } = sessionSort();
    return [...data].sort((a, b) =>
      compareValues(
        a[col as keyof SessionCostRow],
        b[col as keyof SessionCostRow],
        asc,
      ),
    );
  });

  // Format helpers
  const fmtTokens = (n: number): string => {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return n.toLocaleString();
  };
  const fmtCost = (n: number): string => `$${n.toFixed(2)}`;
  const fmtPct = (n: number): string => `${(n * 100).toFixed(0)}%`;
  const fmtActive = (ts: number): string => {
    const now = Date.now() / 1000;
    const diff = now - ts;
    if (diff < 60) return "<1m";
    if (diff < 3600) return `${Math.floor(diff / 60)}m`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h`;
    return `${Math.floor(diff / 86400)}d`;
  };

  // Provider color mapping (CSS variable names)
  const providerColor = (key: string): string => `var(--${key})`;

  // Daily chart: group daily_usage by date, stack by provider
  const dailyChartData = createMemo(() => {
    const daily = stats()?.daily_usage ?? [];
    const byDate = new Map<string, Map<string, number>>();
    let maxTokens = 0;
    for (const d of daily) {
      if (!byDate.has(d.date)) byDate.set(d.date, new Map());
      byDate.get(d.date)!.set(d.provider, d.tokens);
    }
    for (const [, providers] of byDate) {
      const total = [...providers.values()].reduce((a, b) => a + b, 0);
      if (total > maxTokens) maxTokens = total;
    }
    const dates = [...byDate.keys()].sort();
    return { dates, byDate, maxTokens };
  });

  // Sort indicator
  const sortIcon = (currentSort: SortState, col: string): string => {
    if (currentSort.col !== col) return "\u25BC";
    return currentSort.asc ? "\u25B2" : "\u25BC";
  };

  // Provider snapshots for chips
  const providerSnapshots = () => listProviderSnapshots();

  // Range options
  const ranges: { days: number | null; label: () => string }[] = [
    { days: 7, label: () => t("usage.range7d") },
    { days: 30, label: () => t("usage.range30d") },
    { days: 90, label: () => t("usage.range90d") },
    { days: null, label: () => t("usage.rangeAll") },
  ];

  return (
    <div class="usage-panel">
      {/* Header */}
      <div class="usage-header">
        <span class="usage-title">{t("usage.title")}</span>
        <div class="usage-range-group">
          <For each={ranges}>
            {(r) => (
              <button
                class={`usage-range-btn${rangeDays() === r.days ? " active" : ""}`}
                onClick={() => setRangeDays(r.days)}
              >
                {r.label()}
              </button>
            )}
          </For>
        </div>
      </div>

      {/* Provider chips */}
      <div class="usage-chips">
        <For each={providerSnapshots()}>
          {(snap) => (
            <button
              class={`usage-chip${selectedProviders().has(snap.key) ? " active" : " inactive"}`}
              style={
                selectedProviders().has(snap.key)
                  ? { background: providerColor(snap.key) }
                  : {}
              }
              onClick={() => toggleProvider(snap.key)}
            >
              {snap.label}
            </button>
          )}
        </For>
      </div>

      {/* Show content or empty state */}
      <Show
        when={stats()}
        fallback={<div class="usage-loading">{t("common.loading")}</div>}
      >
        {(data) => (
          <>
            <Show
              when={data().total_turns > 0}
              fallback={
                <div class="usage-empty">
                  <Show
                    when={(sessionCount() ?? 0) > 0}
                    fallback={
                      <p class="usage-empty-text">{t("usage.noData")}</p>
                    }
                  >
                    <p class="usage-empty-text">{t("usage.rebuildHint")}</p>
                  </Show>
                </div>
              }
            >
              {/* Summary */}
              <div class="usage-summary">
                <div class="usage-cost-hero">{fmtCost(data().total_cost)}</div>
                <div class="usage-cost-detail">
                  {data().total_sessions} {t("usage.sessions")} &middot;{" "}
                  {data().total_turns.toLocaleString()} {t("usage.turns")}{" "}
                  &middot;{" "}
                  {fmtTokens(
                    data().total_input_tokens +
                      data().total_output_tokens +
                      data().total_cache_read_tokens +
                      data().total_cache_write_tokens,
                  )}{" "}
                  {t("usage.tokens")} &middot;{" "}
                  <span class="usage-cache-hit">
                    {fmtPct(data().cache_hit_rate)} {t("usage.cacheHit")}
                  </span>
                </div>
              </div>

              {/* Daily chart */}
              <Show when={dailyChartData().dates.length > 0}>
                <div class="usage-chart-section">
                  <div class="usage-chart-wrap">
                    <div class="usage-chart-title">{t("usage.dailyUsage")}</div>
                    <div class="usage-daily-bars">
                      <For each={dailyChartData().dates}>
                        {(date) => {
                          const providers = dailyChartData().byDate.get(date)!;
                          const max = dailyChartData().maxTokens;
                          return (
                            <div class="usage-bar-col">
                              <For each={[...providers.entries()].reverse()}>
                                {([provider, tokens]) => (
                                  <div
                                    class="usage-bar-seg"
                                    style={{
                                      height: `${Math.max(2, (tokens / max) * 100)}%`,
                                      background: providerColor(provider),
                                    }}
                                  />
                                )}
                              </For>
                            </div>
                          );
                        }}
                      </For>
                    </div>
                    <div class="usage-bar-labels">
                      <For each={dailyChartData().dates}>
                        {(date) => <span>{date.slice(5)}</span>}
                      </For>
                    </div>
                  </div>
                </div>
              </Show>

              {/* Cost by Model table */}
              <div class="usage-table-section">
                <div class="usage-table-title">{t("usage.costByModel")}</div>
                <table class="usage-table">
                  <thead>
                    <tr>
                      <th>{t("usage.model")}</th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "turns")}
                      >
                        {t("usage.turns")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "turns")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "input_tokens")}
                      >
                        {t("usage.input")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "input_tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() =>
                          toggleSort(setModelSort, "output_tokens")
                        }
                      >
                        {t("usage.output")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "output_tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "cache_tokens")}
                      >
                        {t("usage.cache")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "cache_tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "cost")}
                      >
                        {t("usage.cost")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "cost")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={sortedModels()}>
                      {(row) => (
                        <tr>
                          <td>
                            <span class="usage-model-tag">
                              {row.model || "(unknown)"}
                            </span>
                          </td>
                          <td class="r">{row.turns.toLocaleString()}</td>
                          <td class="r">{fmtTokens(row.input_tokens)}</td>
                          <td class="r">{fmtTokens(row.output_tokens)}</td>
                          <td class="r">{fmtTokens(row.cache_tokens)}</td>
                          <td class="r usage-cost-val">{fmtCost(row.cost)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>

              {/* Cost by Project table */}
              <div class="usage-table-section">
                <div class="usage-table-title">{t("usage.costByProject")}</div>
                <table class="usage-table">
                  <thead>
                    <tr>
                      <th>{t("usage.project")}</th>
                      <th>{t("usage.provider")}</th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "sessions")}
                      >
                        {t("usage.sessions")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "sessions")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "turns")}
                      >
                        {t("usage.turns")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "turns")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "tokens")}
                      >
                        {t("usage.tokens")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "cost")}
                      >
                        {t("usage.cost")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "cost")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={sortedProjects()}>
                      {(row) => (
                        <tr>
                          <td class="usage-project-name">{row.project}</td>
                          <td class="usage-provider-cell">
                            <span
                              class="usage-provider-dot"
                              style={{
                                background: providerColor(row.provider),
                              }}
                            />
                            {row.provider}
                          </td>
                          <td class="r">{row.sessions}</td>
                          <td class="r">{row.turns.toLocaleString()}</td>
                          <td class="r">{fmtTokens(row.tokens)}</td>
                          <td class="r usage-cost-val">{fmtCost(row.cost)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>

              {/* Recent Sessions table */}
              <div class="usage-table-section">
                <div class="usage-table-title">{t("usage.recentSessions")}</div>
                <table class="usage-table">
                  <thead>
                    <tr>
                      <th>{t("usage.project")}</th>
                      <th>{t("usage.provider")}</th>
                      <th>{t("usage.model")}</th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "updated_at")}
                      >
                        {t("usage.active")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "updated_at")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "turns")}
                      >
                        {t("usage.turns")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "turns")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "tokens")}
                      >
                        {t("usage.tokens")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "cost")}
                      >
                        {t("usage.cost")}{" "}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "cost")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={sortedSessions()}>
                      {(row) => (
                        <tr>
                          <td class="usage-project-name">{row.project}</td>
                          <td class="usage-provider-cell">
                            <span
                              class="usage-provider-dot"
                              style={{
                                background: providerColor(row.provider),
                              }}
                            />
                            {row.provider}
                          </td>
                          <td>
                            <span class="usage-model-tag">
                              {row.model || "\u2014"}
                            </span>
                          </td>
                          <td class="r usage-dim">
                            {fmtActive(row.updated_at)}
                          </td>
                          <td class="r">{row.turns.toLocaleString()}</td>
                          <td class="r">{fmtTokens(row.tokens)}</td>
                          <td class="r usage-cost-val">{fmtCost(row.cost)}</td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>

            {/* Footer */}
            <div class="usage-footer">{t("usage.pricingNote")}</div>
          </>
        )}
      </Show>
    </div>
  );
}
