import {
  createSignal,
  createResource,
  createMemo,
  createEffect,
  onCleanup,
  onMount,
  For,
  Show,
} from "solid-js";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useI18n } from "../i18n/index";
import {
  getPricingCatalogStatus,
  startRefreshUsage,
  getIndexStats,
  getUsageStats,
  getSessionCount,
  refreshPricingCatalog,
} from "../lib/tauri";
import {
  getProviderSnapshotVersion,
  listProviderSnapshots,
  refreshProviderSnapshots,
} from "../stores/providerSnapshots";
import { ConfirmDialog } from "./ConfirmDialog";
import { toast, toastError, toastInfo } from "../stores/toast";
import { formatLocalDateTime, shortenHomePath } from "../lib/formatters";
import {
  buildDailyChartData,
  buildHoveredDaySummary,
  compareUsageValues,
  filterScannedProviderSnapshots,
  makeEmptyUsageStats,
  ROW_LIMIT_OPTIONS,
  totalUsageTokens,
  trendPercent,
  type ChartMetric,
  type HoveredDaySummary,
  type UsageSortState,
} from "../lib/usage";
import type {
  MaintenanceEvent,
  MaintenanceJob,
  ModelCost,
  PricingCatalogStatus,
  ProjectCost,
  SessionCostRow,
} from "../lib/types";

type LimitOption = 10 | 25 | 50 | 100;

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
} as const;

export function UsagePanel() {
  const { t } = useI18n();

  const [rangeDays, setRangeDays] = createSignal<number | null>(7);
  const [selectedProviders, setSelectedProviders] = createSignal<Set<string>>(
    new Set(),
  );
  const [didInitProviders, setDidInitProviders] = createSignal(false);
  const [providerSelectionTouched, setProviderSelectionTouched] =
    createSignal(false);
  const [projectLimit, setProjectLimit] = createSignal<LimitOption>(10);
  const [sessionLimit, setSessionLimit] = createSignal<LimitOption>(10);
  const [hoveredDate, setHoveredDate] = createSignal<string | null>(null);
  const [chartMetric, setChartMetric] = createSignal<ChartMetric>("tokens");
  const [showClearUsageConfirm, setShowClearUsageConfirm] = createSignal(false);
  const [isRefreshingPricing, setIsRefreshingPricing] = createSignal(false);
  const [activeMaintenanceJob, setActiveMaintenanceJob] =
    createSignal<MaintenanceJob | null>(null);

  const providerSnapshots = createMemo(() => listProviderSnapshots());
  const scannedProviderSnapshots = createMemo(() =>
    filterScannedProviderSnapshots(providerSnapshots()),
  );
  const scannedProviderKeys = createMemo(() =>
    scannedProviderSnapshots().map((snapshot) => snapshot.key),
  );
  const providerSnapshotMap = createMemo(
    () =>
      new Map(providerSnapshots().map((snapshot) => [snapshot.key, snapshot])),
  );

  createEffect(() => {
    const keys = scannedProviderKeys();
    const snapshotsLoaded = getProviderSnapshotVersion() > 0;
    if (!snapshotsLoaded && keys.length === 0) return;
    if (!providerSelectionTouched()) {
      setSelectedProviders(new Set(keys));
    }
    setDidInitProviders(true);
  });

  const selectedProviderKeys = createMemo(() => {
    const selected = selectedProviders();
    return scannedProviderKeys().filter((key) => selected.has(key));
  });
  const allProvidersSelected = createMemo(
    () =>
      scannedProviderKeys().length > 0 &&
      selectedProviderKeys().length === scannedProviderKeys().length,
  );

  const toggleProvider = (key: string) => {
    setProviderSelectionTouched(true);
    setSelectedProviders((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const selectAllProviders = () => {
    setProviderSelectionTouched(true);
    if (allProvidersSelected()) {
      setSelectedProviders(new Set<string>());
      return;
    }
    setSelectedProviders(new Set<string>(scannedProviderKeys()));
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

  const [sessionCount, { refetch: refetchSessionCount }] = createResource(() =>
    getSessionCount(),
  );
  const [indexStats, { refetch: refetchIndexStats }] = createResource(
    async () => {
      try {
        return await getIndexStats();
      } catch {
        return {
          session_count: 0,
          db_size_bytes: 0,
          last_index_time: "",
          usage_last_refreshed_at: "",
        };
      }
    },
  );
  const [pricingStatus, { refetch: refetchPricingStatus }] =
    createResource<PricingCatalogStatus>(async () => {
      try {
        return await getPricingCatalogStatus();
      } catch {
        return { updated_at: null, model_count: 0 };
      }
    });

  let unlistenMaintenance: UnlistenFn | undefined;
  const handleUsageDataChanged = () => {
    void refreshProviderSnapshots();
    void refetchStats();
    void refetchSessionCount();
    void refetchPricingStatus();
    void refetchIndexStats();
  };

  onMount(async () => {
    window.addEventListener("usage-data-changed", handleUsageDataChanged);
    window.addEventListener("focus", handleUsageDataChangedIfStale);
    document.addEventListener(
      "visibilitychange",
      handleUsageDataChangedIfStale,
    );
    unlistenMaintenance = await listen<MaintenanceEvent>(
      "maintenance-status",
      (event) => {
        const payload = event.payload;
        if (payload.phase === "started") {
          setActiveMaintenanceJob(payload.job);
          return;
        }
        if (
          activeMaintenanceJob() === payload.job &&
          (payload.phase === "finished" || payload.phase === "failed")
        ) {
          setActiveMaintenanceJob(null);
        }
      },
    );
  });

  onCleanup(() => {
    window.removeEventListener("usage-data-changed", handleUsageDataChanged);
    window.removeEventListener("focus", handleUsageDataChangedIfStale);
    document.removeEventListener(
      "visibilitychange",
      handleUsageDataChangedIfStale,
    );
    unlistenMaintenance?.();
  });

  const [modelSort, setModelSort] = createSignal<UsageSortState>({
    col: "cost",
    asc: false,
  });
  const [projectSort, setProjectSort] = createSignal<UsageSortState>({
    col: "cost",
    asc: false,
  });
  const [sessionSort, setSessionSort] = createSignal<UsageSortState>({
    col: "updated_at",
    asc: false,
  });

  const toggleSort = (
    setter: (fn: (prev: UsageSortState) => UsageSortState) => void,
    col: string,
  ) => {
    setter((prev: UsageSortState) => ({
      col,
      asc: prev.col === col ? !prev.asc : false,
    }));
  };

  const sortedModels = createMemo(() => {
    const data = stats()?.model_costs ?? [];
    const { col, asc } = modelSort();
    return [...data].sort((a, b) =>
      compareUsageValues(
        a[col as keyof ModelCost],
        b[col as keyof ModelCost],
        asc,
      ),
    );
  });

  const sortedProjects = createMemo(() => {
    const data = stats()?.project_costs ?? [];
    const { col, asc } = projectSort();
    return [...data].sort((a, b) =>
      compareUsageValues(
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
      compareUsageValues(
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
  const fmtChartValue = (n: number): string =>
    chartMetric() === "cost" ? fmtCost(n) : fmtTokens(n);
  const fmtTrend = (pct: number | null): string => {
    if (pct === null) return "";
    const abs = Math.abs(pct * 100);
    const arrow = pct > 0 ? "\u2191" : pct < 0 ? "\u2193" : "";
    return `${arrow} ${abs.toFixed(0)}%`;
  };
  const trendClass = (
    pct: number | null,
    invertColor: boolean = false,
  ): string => {
    if (pct === null) return "";
    if (pct > 0) return invertColor ? "usage-trend-down" : "usage-trend-up";
    if (pct < 0) return invertColor ? "usage-trend-up" : "usage-trend-down";
    return "";
  };
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
  const formatProjectPath = (projectPath: string): string =>
    shortenHomePath(projectPath || t("common.unknown"));

  const totalTokens = createMemo(() => {
    return totalUsageTokens(stats());
  });

  const dailyChartData = createMemo(() => {
    return buildDailyChartData(
      stats()?.daily_usage ?? [],
      selectedProviderKeys(),
      chartMetric(),
    );
  });

  const hoveredDaySummary = createMemo<HoveredDaySummary | null>(() => {
    return buildHoveredDaySummary(
      hoveredDate(),
      dailyChartData(),
      providerInfo,
    );
  });

  const topModels = createMemo(() => sortedModels().slice(0, 4));
  const maxTopModelCost = createMemo(() => topModels()[0]?.cost ?? 0);

  const activeRangeLabel = createMemo(() => {
    switch (rangeDays()) {
      case 1:
        return t("usage.rangeToday");
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
    if (scannedProviderKeys().length === 0) return t("usage.noData");
    if (selectedProviderKeys().length === 0) return t("usage.selectProvider");
    if (showRebuildHint()) return t("usage.rebuildHint");
    if ((sessionCount() ?? 0) === 0) return t("usage.noData");
    return t("usage.noResults");
  });

  const sortIcon = (currentSort: UsageSortState, col: string): string => {
    if (currentSort.col !== col) return "↕";
    return currentSort.asc ? "↑" : "↓";
  };

  const formattedPricingUpdatedAt = createMemo(() => {
    const updatedAt = pricingStatus()?.updated_at;
    return updatedAt
      ? formatLocalDateTime(updatedAt)
      : t("settings.pricingNotFetched");
  });

  const formattedUsageUpdatedAt = createMemo(() => {
    const updatedAt = indexStats()?.usage_last_refreshed_at;
    return updatedAt ? formatLocalDateTime(updatedAt) : t("usage.notRefreshed");
  });

  const maintenanceStatusText = createMemo(() => {
    const job = activeMaintenanceJob();
    if (job === "refresh_usage") return t("usage.refreshUsageRunning");
    if (job === "rebuild_index") return t("usage.rebuildRunning");
    return t("usage.usageReady");
  });

  const totalCostTrend = createMemo(() =>
    trendPercent(stats()?.total_cost ?? 0, stats()?.prev_period, "total_cost"),
  );

  const summaryStats = createMemo(() => {
    const data = stats();
    return [
      {
        label: t("usage.sessions"),
        value: (data?.total_sessions ?? 0).toLocaleString(),
        trend: trendPercent(
          data?.total_sessions ?? 0,
          data?.prev_period,
          "total_sessions",
        ),
      },
      {
        label: t("usage.turns"),
        value: (data?.total_turns ?? 0).toLocaleString(),
        trend: trendPercent(
          data?.total_turns ?? 0,
          data?.prev_period,
          "total_turns",
        ),
      },
      {
        label: t("usage.tokens"),
        value: fmtTokens(totalTokens()),
        trend: trendPercent(totalTokens(), data?.prev_period, "total_tokens"),
      },
      {
        label: t("usage.cacheHit"),
        value: fmtPct(data?.cache_hit_rate ?? 0),
        trend: null,
      },
    ];
  });

  const tokenBreakdown = createMemo(() => {
    const data = stats();
    const tokenTotal = totalTokens();
    return [
      {
        label: t("usage.input"),
        value: fmtTokens(data?.total_input_tokens ?? 0),
        share:
          tokenTotal > 0
            ? fmtPct((data?.total_input_tokens ?? 0) / tokenTotal)
            : "0%",
      },
      {
        label: t("usage.output"),
        value: fmtTokens(data?.total_output_tokens ?? 0),
        share:
          tokenTotal > 0
            ? fmtPct((data?.total_output_tokens ?? 0) / tokenTotal)
            : "0%",
      },
      {
        label: t("usage.cacheRead"),
        value: fmtTokens(data?.total_cache_read_tokens ?? 0),
        share:
          tokenTotal > 0
            ? fmtPct((data?.total_cache_read_tokens ?? 0) / tokenTotal)
            : "0%",
      },
      {
        label: t("usage.cacheWrite"),
        value: fmtTokens(data?.total_cache_write_tokens ?? 0),
        share:
          tokenTotal > 0
            ? fmtPct((data?.total_cache_write_tokens ?? 0) / tokenTotal)
            : "0%",
      },
    ];
  });

  function handleUsageDataChangedIfStale() {
    if (document.visibilityState === "hidden") return;
    const usageRefreshedAt = indexStats()?.usage_last_refreshed_at;
    if (!usageRefreshedAt) return;
    const parsed = Date.parse(usageRefreshedAt);
    if (!Number.isNaN(parsed) && Date.now() - parsed < 5 * 60 * 1000) return;
    handleUsageDataChanged();
  }

  const ranges: { days: number | null; label: () => string }[] = [
    { days: 1, label: () => t("usage.rangeToday") },
    { days: 7, label: () => t("usage.range7d") },
    { days: 30, label: () => t("usage.range30d") },
    { days: 90, label: () => t("usage.range90d") },
    { days: null, label: () => t("usage.rangeAll") },
  ];

  async function handleRefreshUsage() {
    try {
      const started = await startRefreshUsage();
      if (!started) {
        toastInfo(t("toast.maintenanceBusy"));
        return;
      }
      setHoveredDate(null);
    } catch (error) {
      toastError(String(error));
    }
  }

  async function handleRefreshPricing() {
    setIsRefreshingPricing(true);
    try {
      await refreshPricingCatalog();
      await refetchPricingStatus();
      toast(t("toast.pricingRefreshOk"));
    } catch (error) {
      toastError(String(error));
    } finally {
      setIsRefreshingPricing(false);
    }
  }

  return (
    <div class="usage-panel">
      <section class="usage-card usage-toolbar-card">
        <div class="usage-toolbar-main">
          <div class="usage-toolbar-copy">
            <div class="usage-title-row">
              <h1 class="usage-title">{t("usage.title")}</h1>
              <span class="usage-subtitle-pill">{activeRangeLabel()}</span>
            </div>
            <div class="usage-toolbar-subline">
              <span class="usage-subtitle">
                {selectedProviderKeys().length} {t("usage.providers")}
              </span>
              <span
                class={`usage-status-pill${activeMaintenanceJob() ? " is-active" : ""}`}
              >
                <span class="usage-status-dot" />
                <span>{maintenanceStatusText()}</span>
              </span>
            </div>
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
              class="usage-action-btn"
              onClick={handleRefreshPricing}
              disabled={
                isRefreshingPricing() || activeMaintenanceJob() !== null
              }
              type="button"
            >
              {isRefreshingPricing()
                ? "..."
                : t("settings.refreshPricingCatalog")}
            </button>
            <button
              class="usage-action-btn usage-action-btn-primary"
              onClick={() => setShowClearUsageConfirm(true)}
              disabled={activeMaintenanceJob() !== null}
              type="button"
            >
              {activeMaintenanceJob() === "refresh_usage"
                ? "..."
                : t("usage.refreshUsage")}
            </button>
          </div>
        </div>

        <div class="usage-toolbar-meta">
          <span class="usage-meta-pill">
            {t("usage.pricingUpdatedShort").replace(
              "{count}",
              String(pricingStatus()?.model_count ?? 0),
            )}
          </span>
          <span class="usage-meta-pill">
            {t("usage.pricingUpdatedAtShort").replace(
              "{updatedAt}",
              formattedPricingUpdatedAt(),
            )}
          </span>
          <span class="usage-meta-pill">
            {t("usage.usageUpdatedShort").replace(
              "{updatedAt}",
              formattedUsageUpdatedAt(),
            )}
          </span>
        </div>

        <div class="usage-chips">
          <button
            class={`usage-chip usage-chip-all${allProvidersSelected() ? " active" : " inactive"}`}
            aria-pressed={allProvidersSelected()}
            onClick={selectAllProviders}
            type="button"
          >
            <span class="usage-chip-label">{t("usage.allProviders")}</span>
            <span class="usage-chip-count">{scannedProviderKeys().length}</span>
          </button>
          <For each={scannedProviderSnapshots()}>
            {(snapshot) => {
              const info = providerInfo(snapshot.key);
              const active = () => selectedProviders().has(snapshot.key);
              const filteredCount = () => {
                const counts = stats()?.provider_session_counts;
                return (
                  counts?.find((c) => c.provider === snapshot.key)?.count ?? 0
                );
              };
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
                  <Show when={filteredCount() > 0}>
                    <span class="usage-chip-count">{filteredCount()}</span>
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
            <div class="usage-content-stack">
              <section class="usage-card usage-summary-card">
                <div class="usage-summary-main">
                  <div class="usage-summary-hero">
                    <span class="usage-overline">{t("usage.estCost")}</span>
                    <div class="usage-cost-row">
                      <div class="usage-cost-hero">
                        {fmtCost(data().total_cost)}
                      </div>
                      <Show when={totalCostTrend() !== null}>
                        <span
                          class={`usage-trend ${trendClass(
                            totalCostTrend(),
                            true,
                          )}`}
                        >
                          {fmtTrend(totalCostTrend())}
                        </span>
                      </Show>
                    </div>
                    <div class="usage-cost-detail">
                      {t("usage.pricingNote")}
                    </div>
                  </div>

                  <div class="usage-summary-kpis">
                    <For each={summaryStats()}>
                      {(item) => (
                        <div class="usage-summary-stat">
                          <span class="usage-kpi-label">{item.label}</span>
                          <strong class="usage-kpi-value">{item.value}</strong>
                          <span class="usage-kpi-sub">
                            <Show
                              when={item.trend !== null}
                              fallback={"\u00A0"}
                            >
                              <span
                                class={`usage-trend ${trendClass(item.trend)}`}
                              >
                                {fmtTrend(item.trend)}
                              </span>
                            </Show>
                          </span>
                        </div>
                      )}
                    </For>
                  </div>
                </div>

                <div class="usage-breakdown-grid">
                  <For each={tokenBreakdown()}>
                    {(item) => (
                      <div class="usage-breakdown-item">
                        <span class="usage-breakdown-label">{item.label}</span>
                        <strong class="usage-breakdown-value">
                          {item.value}
                        </strong>
                        <span class="usage-breakdown-pct">{item.share}</span>
                      </div>
                    )}
                  </For>
                </div>

                <div class="usage-summary-notes">
                  <span class="usage-note-pill">
                    {t("usage.rebuildKeepsSessions")}
                  </span>
                  <span class="usage-note-pill">
                    {t("usage.pricingSourceNote")}
                  </span>
                </div>
              </section>

              <div class="usage-overview-grid">
                <section class="usage-card usage-chart-card">
                  <div class="usage-section-header">
                    <div class="usage-section-title-row">
                      <div class="usage-chart-heading">
                        <div class="usage-section-title">
                          {t("usage.dailyUsage")}
                        </div>
                        <div class="usage-section-subtitle">
                          {activeRangeLabel()}
                        </div>
                        <div class="usage-metric-toggle">
                          <button
                            class={`usage-metric-btn${chartMetric() === "tokens" ? " active" : ""}`}
                            aria-pressed={chartMetric() === "tokens"}
                            onClick={() => setChartMetric("tokens")}
                            type="button"
                          >
                            {t("usage.tokens")}
                          </button>
                          <button
                            class={`usage-metric-btn${chartMetric() === "cost" ? " active" : ""}`}
                            aria-pressed={chartMetric() === "cost"}
                            onClick={() => setChartMetric("cost")}
                            type="button"
                          >
                            {t("usage.cost")}
                          </button>
                        </div>
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
                              {fmtChartValue(summary().total)}
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
                                    <strong>
                                      {fmtChartValue(entry.value)}
                                    </strong>
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
                        <div class="usage-daily-bars">
                          <For each={dailyChartData().dates}>
                            {(date) => {
                              const providers =
                                dailyChartData().byDate.get(date)!;
                              const max = dailyChartData().maxValue;
                              const active = () => hoveredDate() === date;
                              return (
                                <button
                                  class={`usage-bar-col${active() ? " active" : ""}`}
                                  onBlur={() => setHoveredDate(null)}
                                  onFocus={() => setHoveredDate(date)}
                                  onMouseEnter={() => setHoveredDate(date)}
                                  onMouseLeave={() => setHoveredDate(null)}
                                  title={`${date} · ${fmtChartValue(
                                    [...providers.values()].reduce(
                                      (sum, value) => sum + value,
                                      0,
                                    ),
                                  )}`}
                                  type="button"
                                >
                                  <For
                                    each={dailyChartData()
                                      .providers.slice()
                                      .reverse()}
                                  >
                                    {(provider) => {
                                      const val = providers.get(provider) ?? 0;
                                      return (
                                        <Show when={val > 0}>
                                          <span
                                            class={`usage-bar-seg${
                                              hoveredDate() && !active()
                                                ? " usage-bar-seg-muted"
                                                : ""
                                            }`}
                                            style={{
                                              height: `${Math.max(
                                                4,
                                                (val / max) * 100,
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
                    <div class="usage-section-subtitle">
                      {t("usage.estCost")}
                    </div>
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
                          onClick={() =>
                            toggleSort(setModelSort, "input_tokens")
                          }
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
                          onClick={() =>
                            toggleSort(setModelSort, "cache_tokens")
                          }
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
                            <td class="r usage-cost-val">
                              {fmtCost(row.cost)}
                            </td>
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
                                    title={formatProjectPath(row.project_path)}
                                  >
                                    {formatProjectPath(row.project_path)}
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
                          onClick={() =>
                            toggleSort(setSessionSort, "updated_at")
                          }
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
                                    title={formatProjectPath(row.project_path)}
                                  >
                                    {formatProjectPath(row.project_path)}
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
            </div>
          </Show>
        )}
      </Show>

      <ConfirmDialog
        open={showClearUsageConfirm()}
        title={t("usage.refreshUsage")}
        message={t("usage.refreshUsageConfirm")}
        confirmLabel={t("usage.refreshUsage")}
        onConfirm={() => {
          setShowClearUsageConfirm(false);
          void handleRefreshUsage();
        }}
        onCancel={() => setShowClearUsageConfirm(false)}
        danger={true}
      />
    </div>
  );
}
