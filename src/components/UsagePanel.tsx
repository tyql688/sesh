import {
  createSignal,
  createResource,
  createMemo,
  createEffect,
  For,
  Show,
} from "solid-js";
import { useI18n } from "../i18n/index";
import { clearUsageStats, getUsageStats, getSessionCount } from "../lib/tauri";
import { listProviderSnapshots } from "../stores/providerSnapshots";
import { ConfirmDialog } from "./ConfirmDialog";
import { toast, toastError } from "../stores/toast";
import type {
  ModelCost,
  ProjectCost,
  SessionCostRow,
  UsageStats,
} from "../lib/types";

type SortState = { col: string; asc: boolean };
type LimitOption = 10 | 25 | 50 | 100;

type HoveredDaySummary = {
  date: string;
  total: number;
  xPercent: number;
  breakdown: Array<{
    provider: string;
    label: string;
    color: string;
    tokens: number;
  }>;
};

const ROW_LIMIT_OPTIONS: LimitOption[] = [10, 25, 50, 100];

const SHORT_PROVIDER_LABELS: Record<string, string> = {
  claude: "Claude",
  "cc-mirror": "CC-Mirror",
  codex: "Codex",
  gemini: "Gemini",
  cursor: "Cursor",
  opencode: "OpenCode",
  kimi: "Kimi",
  qwen: "Qwen",
  copilot: "Copilot",
};

function makeEmptyUsageStats(): UsageStats {
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

export function UsagePanel() {
  const { t } = useI18n();

  const [rangeDays, setRangeDays] = createSignal<number | null>(7);
  const [selectedProviders, setSelectedProviders] = createSignal<Set<string>>(
    new Set(),
  );
  const [didInitProviders, setDidInitProviders] = createSignal(false);
  const [projectLimit, setProjectLimit] = createSignal<LimitOption>(10);
  const [sessionLimit, setSessionLimit] = createSignal<LimitOption>(10);
  const [hoveredDate, setHoveredDate] = createSignal<string | null>(null);
  const [showClearUsageConfirm, setShowClearUsageConfirm] = createSignal(false);

  const providerSnapshots = createMemo(() => listProviderSnapshots());
  const existingProviderSnapshots = createMemo(() =>
    providerSnapshots().filter((snapshot) => snapshot.exists),
  );
  const existingProviderKeys = createMemo(() =>
    existingProviderSnapshots().map((snapshot) => snapshot.key),
  );
  const providerSnapshotMap = createMemo(
    () =>
      new Map(providerSnapshots().map((snapshot) => [snapshot.key, snapshot])),
  );

  createEffect(() => {
    if (didInitProviders()) return;
    if (existingProviderSnapshots().length === 0) return;
    setSelectedProviders(new Set(existingProviderKeys()));
    setDidInitProviders(true);
  });

  const selectedProviderKeys = createMemo(() => {
    const selected = selectedProviders();
    return existingProviderKeys().filter((key) => selected.has(key));
  });
  const allProvidersSelected = createMemo(
    () =>
      existingProviderKeys().length > 0 &&
      selectedProviderKeys().length === existingProviderKeys().length,
  );

  const toggleProvider = (key: string) => {
    setSelectedProviders((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const selectAllProviders = () => {
    if (allProvidersSelected()) {
      setSelectedProviders(new Set<string>());
      return;
    }
    setSelectedProviders(new Set<string>(existingProviderKeys()));
  };

  const [stats, { refetch: refetchStats }] = createResource(
    () =>
      didInitProviders()
        ? { providers: selectedProviderKeys(), range: rangeDays() }
        : null,
    async (params) => {
      if (!params || params.providers.length === 0) {
        return makeEmptyUsageStats();
      }
      return getUsageStats(params.providers, params.range);
    },
  );

  const [sessionCount] = createResource(() => getSessionCount());

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

  const toggleSort = (
    setter: (fn: (prev: SortState) => SortState) => void,
    col: string,
  ) => {
    setter((prev: SortState) => ({
      col,
      asc: prev.col === col ? !prev.asc : false,
    }));
  };

  const compareValues = (va: unknown, vb: unknown, asc: boolean): number => {
    if (typeof va === "string" && typeof vb === "string") {
      return asc ? va.localeCompare(vb) : vb.localeCompare(va);
    }
    const na = typeof va === "number" ? va : 0;
    const nb = typeof vb === "number" ? vb : 0;
    return asc ? na - nb : nb - na;
  };

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

  const visibleProjects = createMemo(() =>
    sortedProjects().slice(0, projectLimit()),
  );
  const visibleSessions = createMemo(() =>
    sortedSessions().slice(0, sessionLimit()),
  );

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

  const providerInfo = (key: string) => {
    const snapshot = providerSnapshotMap().get(key as never);
    return {
      color: snapshot?.color ?? `var(--${key})`,
      label: SHORT_PROVIDER_LABELS[key] ?? snapshot?.label ?? key,
      fullLabel: snapshot?.label ?? key,
    };
  };

  const formatModelName = (model: string): string =>
    model.trim().length > 0 ? model : t("common.unknown");
  const formatProjectName = (project: string, projectPath: string): string => {
    if (project.trim().length > 0) return project;
    const name = projectPath.split(/[\\/]/).filter(Boolean).at(-1);
    return name || t("common.unknown");
  };

  const totalTokens = createMemo(() => {
    const data = stats();
    if (!data) return 0;
    return (
      data.total_input_tokens +
      data.total_output_tokens +
      data.total_cache_read_tokens +
      data.total_cache_write_tokens
    );
  });

  const dailyChartData = createMemo(() => {
    const daily = stats()?.daily_usage ?? [];
    const byDate = new Map<string, Map<string, number>>();
    let maxTokens = 0;

    for (const item of daily) {
      if (!byDate.has(item.date)) byDate.set(item.date, new Map());
      byDate.get(item.date)!.set(item.provider, item.tokens);
    }

    for (const providerMap of byDate.values()) {
      const total = [...providerMap.values()].reduce(
        (sum, value) => sum + value,
        0,
      );
      if (total > maxTokens) maxTokens = total;
    }

    const providers = selectedProviderKeys().filter((key) =>
      [...byDate.values()].some(
        (providerMap) => (providerMap.get(key) ?? 0) > 0,
      ),
    );

    return {
      dates: [...byDate.keys()].sort(),
      byDate,
      providers,
      maxTokens: maxTokens || 1,
    };
  });

  const hoveredDaySummary = createMemo<HoveredDaySummary | null>(() => {
    const date = hoveredDate();
    if (!date) return null;
    const dateIndex = dailyChartData().dates.indexOf(date);
    if (dateIndex === -1) return null;

    const providerMap = dailyChartData().byDate.get(date);
    if (!providerMap) return null;

    const breakdown = dailyChartData()
      .providers.map((provider) => {
        const tokens = providerMap.get(provider) ?? 0;
        const info = providerInfo(provider);
        return {
          provider,
          label: info.label,
          color: info.color,
          tokens,
        };
      })
      .filter((entry) => entry.tokens > 0)
      .sort((left, right) => right.tokens - left.tokens);

    return {
      date,
      total: breakdown.reduce((sum, entry) => sum + entry.tokens, 0),
      xPercent: ((dateIndex + 0.5) / dailyChartData().dates.length) * 100,
      breakdown,
    };
  });

  const topModels = createMemo(() => sortedModels().slice(0, 4));
  const maxTopModelCost = createMemo(() => topModels()[0]?.cost ?? 0);

  const activeRangeLabel = createMemo(() => {
    switch (rangeDays()) {
      case 7:
        return t("usage.range7d");
      case 30:
        return t("usage.range30d");
      case 90:
        return t("usage.range90d");
      default:
        return t("usage.rangeAll");
    }
  });

  const showRebuildHint = createMemo(() => {
    const data = stats();
    if (!data || data.total_turns > 0) return false;
    if (selectedProviderKeys().length === 0) return false;
    return (
      rangeDays() === null &&
      allProvidersSelected() &&
      (sessionCount() ?? 0) > 0
    );
  });

  const emptyMessage = createMemo(() => {
    if (selectedProviderKeys().length === 0) return t("usage.selectProvider");
    if (showRebuildHint()) return t("usage.rebuildHint");
    if ((sessionCount() ?? 0) === 0) return t("usage.noData");
    return t("usage.noResults");
  });

  const sortIcon = (currentSort: SortState, col: string): string => {
    if (currentSort.col !== col) return "↕";
    return currentSort.asc ? "↑" : "↓";
  };

  const ranges: { days: number | null; label: () => string }[] = [
    { days: 7, label: () => t("usage.range7d") },
    { days: 30, label: () => t("usage.range30d") },
    { days: 90, label: () => t("usage.range90d") },
    { days: null, label: () => t("usage.rangeAll") },
  ];

  async function handleClearUsageStats() {
    try {
      await clearUsageStats(selectedProviderKeys());
      setHoveredDate(null);
      await refetchStats();
      toast(t("toast.clearUsageOk"));
    } catch (error) {
      toastError(String(error));
    }
  }

  return (
    <div class="usage-panel">
      <section class="usage-card usage-toolbar-card">
        <div class="usage-toolbar-main">
          <div>
            <div class="usage-title-row">
              <h1 class="usage-title">{t("usage.title")}</h1>
              <span class="usage-subtitle-pill">{activeRangeLabel()}</span>
            </div>
            <p class="usage-subtitle">
              {selectedProviderKeys().length} {t("usage.providers")}
            </p>
            <p class="usage-note">{t("usage.rebuildKeepsSessions")}</p>
            <p class="usage-note">{t("usage.pricingSourceNote")}</p>
          </div>
          <div class="usage-toolbar-actions">
            <div class="usage-range-group">
              <For each={ranges}>
                {(range) => (
                  <button
                    class={`usage-range-btn${rangeDays() === range.days ? " active" : ""}`}
                    aria-pressed={rangeDays() === range.days}
                    onClick={() => setRangeDays(range.days)}
                    type="button"
                  >
                    {range.label()}
                  </button>
                )}
              </For>
            </div>
            <button
              class="usage-secondary-btn"
              onClick={() => setShowClearUsageConfirm(true)}
              type="button"
            >
              {t("usage.clearUsage")}
            </button>
          </div>
        </div>

        <div class="usage-chips">
          <button
            class={`usage-chip usage-chip-all${allProvidersSelected() ? " active" : " inactive"}`}
            aria-pressed={allProvidersSelected()}
            onClick={selectAllProviders}
            type="button"
          >
            <span class="usage-chip-label">{t("usage.allProviders")}</span>
            <span class="usage-chip-count">
              {existingProviderKeys().length}
            </span>
          </button>
          <For each={existingProviderSnapshots()}>
            {(snapshot) => {
              const info = providerInfo(snapshot.key);
              const active = () => selectedProviders().has(snapshot.key);
              return (
                <button
                  class={`usage-chip${active() ? " active" : " inactive"}`}
                  aria-pressed={active()}
                  onClick={() => toggleProvider(snapshot.key)}
                  style={{ "--provider-accent": info.color }}
                  title={info.fullLabel}
                  type="button"
                >
                  <span
                    class="usage-chip-dot"
                    style={{ background: info.color }}
                  />
                  <span class="usage-chip-label">{info.label}</span>
                  <Show when={snapshot.session_count > 0}>
                    <span class="usage-chip-count">
                      {snapshot.session_count}
                    </span>
                  </Show>
                </button>
              );
            }}
          </For>
        </div>
      </section>

      <Show
        when={stats()}
        fallback={<div class="usage-loading">{t("common.loading")}</div>}
      >
        {(data) => (
          <Show
            when={data().total_turns > 0}
            fallback={
              <section class="usage-card usage-empty">
                <p class="usage-empty-text">{emptyMessage()}</p>
              </section>
            }
          >
            <div class="usage-summary-grid">
              <section class="usage-card usage-hero-card">
                <span class="usage-overline">{t("usage.estCost")}</span>
                <div class="usage-cost-hero">{fmtCost(data().total_cost)}</div>
                <div class="usage-cost-detail">
                  {fmtTokens(totalTokens())} {t("usage.tokens")} ·{" "}
                  {t("usage.pricingNote")}
                </div>
              </section>

              <div class="usage-kpi-grid">
                <div class="usage-card usage-kpi-card">
                  <span class="usage-kpi-label">{t("usage.sessions")}</span>
                  <strong class="usage-kpi-value">
                    {data().total_sessions}
                  </strong>
                </div>
                <div class="usage-card usage-kpi-card">
                  <span class="usage-kpi-label">{t("usage.turns")}</span>
                  <strong class="usage-kpi-value">
                    {data().total_turns.toLocaleString()}
                  </strong>
                </div>
                <div class="usage-card usage-kpi-card">
                  <span class="usage-kpi-label">{t("usage.tokens")}</span>
                  <strong class="usage-kpi-value">
                    {fmtTokens(totalTokens())}
                  </strong>
                </div>
                <div class="usage-card usage-kpi-card">
                  <span class="usage-kpi-label">{t("usage.cacheHit")}</span>
                  <strong class="usage-kpi-value">
                    {fmtPct(data().cache_hit_rate)}
                  </strong>
                </div>
              </div>
            </div>

            <div class="usage-overview-grid">
              <section class="usage-card usage-chart-card">
                <div class="usage-section-header">
                  <div>
                    <div class="usage-section-title">
                      {t("usage.dailyUsage")}
                    </div>
                    <div class="usage-section-subtitle">
                      {activeRangeLabel()}
                    </div>
                  </div>
                  <div class="usage-chart-inspector">
                    <Show
                      when={hoveredDaySummary()}
                      fallback={
                        <div class="usage-chart-hint">
                          {t("usage.hoverHint")}
                        </div>
                      }
                    >
                      {(summary) => (
                        <>
                          <div class="usage-chart-inspector-date">
                            {summary().date}
                          </div>
                          <div class="usage-chart-inspector-total">
                            {fmtTokens(summary().total)} {t("usage.tokens")}
                          </div>
                          <div class="usage-chart-inspector-breakdown">
                            <For each={summary().breakdown}>
                              {(entry) => (
                                <span class="usage-chart-inspector-item">
                                  <span
                                    class="usage-provider-dot"
                                    style={{ background: entry.color }}
                                  />
                                  {entry.label}
                                  <strong>{fmtTokens(entry.tokens)}</strong>
                                </span>
                              )}
                            </For>
                          </div>
                        </>
                      )}
                    </Show>
                  </div>
                </div>

                <Show when={dailyChartData().dates.length > 0}>
                  <>
                    <div class="usage-chart-wrap">
                      <Show when={hoveredDaySummary()}>
                        {(summary) => (
                          <div
                            class="usage-chart-tooltip"
                            style={{ left: `${summary().xPercent}%` }}
                          >
                            <div class="usage-chart-tooltip-date">
                              {summary().date}
                            </div>
                            <div class="usage-chart-tooltip-total">
                              {fmtTokens(summary().total)} {t("usage.tokens")}
                            </div>
                            <div class="usage-chart-tooltip-breakdown">
                              <For each={summary().breakdown}>
                                {(entry) => (
                                  <span class="usage-chart-tooltip-item">
                                    <span
                                      class="usage-provider-dot"
                                      style={{ background: entry.color }}
                                    />
                                    {entry.label} · {fmtTokens(entry.tokens)}
                                  </span>
                                )}
                              </For>
                            </div>
                          </div>
                        )}
                      </Show>
                      <div class="usage-daily-bars">
                        <For each={dailyChartData().dates}>
                          {(date) => {
                            const providers =
                              dailyChartData().byDate.get(date)!;
                            const max = dailyChartData().maxTokens;
                            const active = () => hoveredDate() === date;
                            return (
                              <button
                                class={`usage-bar-col${active() ? " active" : ""}`}
                                onBlur={() => setHoveredDate(null)}
                                onFocus={() => setHoveredDate(date)}
                                onMouseEnter={() => setHoveredDate(date)}
                                onMouseLeave={() => setHoveredDate(null)}
                                title={`${date} · ${fmtTokens(
                                  [...providers.values()].reduce(
                                    (sum, value) => sum + value,
                                    0,
                                  ),
                                )} ${t("usage.tokens")}`}
                                type="button"
                              >
                                <For
                                  each={dailyChartData()
                                    .providers.slice()
                                    .reverse()}
                                >
                                  {(provider) => {
                                    const tokens = providers.get(provider) ?? 0;
                                    return (
                                      <Show when={tokens > 0}>
                                        <span
                                          class={`usage-bar-seg${
                                            hoveredDate() && !active()
                                              ? " usage-bar-seg-muted"
                                              : ""
                                          }`}
                                          style={{
                                            height: `${Math.max(
                                              4,
                                              (tokens / max) * 100,
                                            )}%`,
                                            background:
                                              providerInfo(provider).color,
                                          }}
                                        />
                                      </Show>
                                    );
                                  }}
                                </For>
                              </button>
                            );
                          }}
                        </For>
                      </div>
                      <div class="usage-bar-labels">
                        <For each={dailyChartData().dates}>
                          {(date) => (
                            <span
                              class={
                                hoveredDate() === date ? "active" : undefined
                              }
                            >
                              {date.slice(5)}
                            </span>
                          )}
                        </For>
                      </div>
                    </div>

                    <div class="usage-legend">
                      <For each={dailyChartData().providers}>
                        {(provider) => (
                          <span class="usage-legend-item">
                            <span
                              class="usage-provider-dot"
                              style={{
                                background: providerInfo(provider).color,
                              }}
                            />
                            {providerInfo(provider).label}
                          </span>
                        )}
                      </For>
                    </div>
                  </>
                </Show>
              </section>

              <section class="usage-card usage-spotlight-card">
                <div class="usage-section-header">
                  <div>
                    <div class="usage-section-title">
                      {t("usage.topModels")}
                    </div>
                    <div class="usage-section-subtitle">
                      {t("usage.costByModel")}
                    </div>
                  </div>
                </div>

                <Show
                  when={topModels().length > 0}
                  fallback={
                    <div class="usage-empty-inline">{t("usage.noData")}</div>
                  }
                >
                  <div class="usage-spotlight-list">
                    <For each={topModels()}>
                      {(row) => (
                        <div class="usage-spotlight-item">
                          <div class="usage-spotlight-meta">
                            <span class="usage-model-tag">
                              {formatModelName(row.model)}
                            </span>
                            <span class="usage-spotlight-tokens">
                              {fmtTokens(
                                row.input_tokens +
                                  row.output_tokens +
                                  row.cache_tokens,
                              )}
                            </span>
                          </div>
                          <div class="usage-spotlight-bar">
                            <div
                              class="usage-spotlight-bar-fill"
                              style={{
                                width: `${Math.max(
                                  8,
                                  maxTopModelCost() > 0
                                    ? (row.cost / maxTopModelCost()) * 100
                                    : 0,
                                )}%`,
                              }}
                            />
                          </div>
                          <div class="usage-spotlight-cost">
                            {fmtCost(row.cost)}
                          </div>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </section>
            </div>

            <section class="usage-card usage-table-card">
              <div class="usage-section-header">
                <div>
                  <div class="usage-section-title">
                    {t("usage.costByModel")}
                  </div>
                  <div class="usage-section-subtitle">{t("usage.estCost")}</div>
                </div>
              </div>
              <div class="usage-table-wrap">
                <table class="usage-table">
                  <thead>
                    <tr>
                      <th>{t("usage.model")}</th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "turns")}
                      >
                        {t("usage.turns")}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "turns")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "input_tokens")}
                      >
                        {t("usage.input")}
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
                        {t("usage.output")}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "output_tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "cache_tokens")}
                      >
                        {t("usage.cache")}
                        <span class="usage-sort-icon">
                          {sortIcon(modelSort(), "cache_tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setModelSort, "cost")}
                      >
                        {t("usage.cost")}
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
                            <div class="usage-model-cell">
                              <span class="usage-model-tag">
                                {formatModelName(row.model)}
                              </span>
                              <Show
                                when={
                                  row.cost === 0 &&
                                  row.input_tokens +
                                    row.output_tokens +
                                    row.cache_tokens >
                                    0
                                }
                              >
                                <span class="usage-price-badge">
                                  {t("usage.unpriced")}
                                </span>
                              </Show>
                            </div>
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
            </section>

            <section class="usage-card usage-table-card">
              <div class="usage-section-header">
                <div>
                  <div class="usage-section-title">
                    {t("usage.costByProject")}
                  </div>
                  <div class="usage-section-subtitle">
                    {Math.min(projectLimit(), sortedProjects().length)}/
                    {sortedProjects().length}
                  </div>
                </div>
                <div class="usage-section-actions">
                  <For each={ROW_LIMIT_OPTIONS}>
                    {(limit) => (
                      <button
                        class={`usage-limit-btn${projectLimit() === limit ? " active" : ""}`}
                        onClick={() => setProjectLimit(limit)}
                        type="button"
                      >
                        {limit}
                      </button>
                    )}
                  </For>
                </div>
              </div>
              <div class="usage-table-wrap">
                <table class="usage-table">
                  <thead>
                    <tr>
                      <th>{t("usage.project")}</th>
                      <th>{t("usage.provider")}</th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "sessions")}
                      >
                        {t("usage.sessions")}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "sessions")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "turns")}
                      >
                        {t("usage.turns")}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "turns")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "tokens")}
                      >
                        {t("usage.tokens")}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setProjectSort, "cost")}
                      >
                        {t("usage.cost")}
                        <span class="usage-sort-icon">
                          {sortIcon(projectSort(), "cost")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={visibleProjects()}>
                      {(row) => {
                        const info = providerInfo(row.provider);
                        return (
                          <tr>
                            <td>
                              <div class="usage-entity-cell">
                                <div class="usage-entity-title">
                                  {formatProjectName(
                                    row.project,
                                    row.project_path,
                                  )}
                                </div>
                                <div
                                  class="usage-entity-subtitle"
                                  title={row.project_path}
                                >
                                  {row.project_path}
                                </div>
                              </div>
                            </td>
                            <td class="usage-provider-cell">
                              <span
                                class="usage-provider-dot"
                                style={{ background: info.color }}
                              />
                              {info.label}
                            </td>
                            <td class="r">{row.sessions}</td>
                            <td class="r">{row.turns.toLocaleString()}</td>
                            <td class="r">{fmtTokens(row.tokens)}</td>
                            <td class="r usage-cost-val">
                              {fmtCost(row.cost)}
                            </td>
                          </tr>
                        );
                      }}
                    </For>
                  </tbody>
                </table>
              </div>
            </section>

            <section class="usage-card usage-table-card">
              <div class="usage-section-header">
                <div>
                  <div class="usage-section-title">
                    {t("usage.recentSessions")}
                  </div>
                  <div class="usage-section-subtitle">
                    {Math.min(sessionLimit(), sortedSessions().length)}/
                    {sortedSessions().length}
                  </div>
                </div>
                <div class="usage-section-actions">
                  <For each={ROW_LIMIT_OPTIONS}>
                    {(limit) => (
                      <button
                        class={`usage-limit-btn${sessionLimit() === limit ? " active" : ""}`}
                        onClick={() => setSessionLimit(limit)}
                        type="button"
                      >
                        {limit}
                      </button>
                    )}
                  </For>
                </div>
              </div>
              <div class="usage-table-wrap">
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
                        {t("usage.active")}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "updated_at")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "turns")}
                      >
                        {t("usage.turns")}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "turns")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "tokens")}
                      >
                        {t("usage.tokens")}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "tokens")}
                        </span>
                      </th>
                      <th
                        class="r"
                        onClick={() => toggleSort(setSessionSort, "cost")}
                      >
                        {t("usage.cost")}
                        <span class="usage-sort-icon">
                          {sortIcon(sessionSort(), "cost")}
                        </span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={visibleSessions()}>
                      {(row) => {
                        const info = providerInfo(row.provider);
                        return (
                          <tr>
                            <td>
                              <div class="usage-entity-cell">
                                <div class="usage-entity-title">
                                  {formatProjectName(
                                    row.project,
                                    row.project_path,
                                  )}
                                </div>
                                <div
                                  class="usage-entity-subtitle"
                                  title={row.project_path}
                                >
                                  {row.project_path}
                                </div>
                              </div>
                            </td>
                            <td class="usage-provider-cell">
                              <span
                                class="usage-provider-dot"
                                style={{ background: info.color }}
                              />
                              {info.label}
                            </td>
                            <td>
                              <span class="usage-model-tag">
                                {formatModelName(row.model)}
                              </span>
                            </td>
                            <td class="r usage-dim">
                              {fmtActive(row.updated_at)}
                            </td>
                            <td class="r">{row.turns.toLocaleString()}</td>
                            <td class="r">{fmtTokens(row.tokens)}</td>
                            <td class="r usage-cost-val">
                              {fmtCost(row.cost)}
                            </td>
                          </tr>
                        );
                      }}
                    </For>
                  </tbody>
                </table>
              </div>
            </section>
          </Show>
        )}
      </Show>

      <ConfirmDialog
        open={showClearUsageConfirm()}
        title={t("usage.clearUsage")}
        message={
          selectedProviderKeys().length > 0
            ? t("usage.clearUsageConfirmSelected").replace(
                "{count}",
                String(selectedProviderKeys().length),
              )
            : t("usage.clearUsageConfirm")
        }
        confirmLabel={t("usage.clearUsage")}
        onConfirm={() => {
          setShowClearUsageConfirm(false);
          void handleClearUsageStats();
        }}
        onCancel={() => setShowClearUsageConfirm(false)}
        danger={true}
      />
    </div>
  );
}
