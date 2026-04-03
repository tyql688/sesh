import { onMount, Show } from "solid-js";
import { useI18n } from "../i18n/index";
import type { Locale } from "../i18n/index";
import { theme, setTheme, applyTheme } from "../stores/theme";
import type { Theme } from "../stores/theme";
import { phase, availableVersion, downloadAndInstall } from "../stores/updater";

export function StatusBar(props: {
  sessionCount: number;
  providerCount: number;
  isIndexing?: boolean;
}) {
  const { t, locale, setLocale } = useI18n();

  onMount(() => {
    applyTheme(theme());
  });

  function cycleTheme() {
    const order: Theme[] = ["light", "dark", "system"];
    const idx = order.indexOf(theme());
    const next = order[(idx + 1) % order.length];
    setTheme(next);
  }

  const themeIcon = () => {
    switch (theme()) {
      case "light":
        return "☀️";
      case "dark":
        return "🌙";
      case "system":
        return "💻";
    }
  };

  const themeLabel = () => {
    switch (theme()) {
      case "light":
        return t("status.themeLight");
      case "dark":
        return t("status.themeDark");
      case "system":
        return t("status.themeSystem");
    }
  };

  const updateLabel = () => {
    switch (phase()) {
      case "available":
        return `↑ v${availableVersion()}`;
      case "downloading":
      case "installing":
        return t("settings.updating");
      case "error":
        return t("settings.updateFailed");
      default:
        return null;
    }
  };

  const isBusy = () =>
    phase() === "downloading" ||
    phase() === "installing" ||
    phase() === "error";

  return (
    <div class="statusbar">
      <div class="statusbar-left">
        <span class={props.isIndexing ? "status-dot-indexing" : "status-dot"} />
        <span>
          {props.isIndexing ? (
            t("status.indexing")
          ) : (
            <>
              {t("status.indexed")} — {props.sessionCount.toLocaleString()}{" "}
              {t("status.sessions")}
            </>
          )}
        </span>
        <span class="status-separator">·</span>
        <span>
          {props.providerCount} {t("status.providers")}
        </span>
      </div>
      <div class="statusbar-right">
        <Show when={updateLabel() !== null}>
          <button
            class={`update-badge${isBusy() ? " busy" : ""}`}
            disabled={isBusy()}
            onClick={() => {
              if (phase() === "available") void downloadAndInstall();
            }}
            title={updateLabel() ?? ""}
          >
            {updateLabel()}
          </button>
        </Show>
        <button class="theme-toggle" onClick={cycleTheme} title={themeLabel()}>
          {themeIcon()}
        </button>
        <span class="locale-toggle">
          <button
            class={`locale-btn${locale() === "en" ? " active" : ""}`}
            onClick={() => setLocale("en" as Locale)}
          >
            EN
          </button>
          <span class="locale-divider">|</span>
          <button
            class={`locale-btn${locale() === "zh" ? " active" : ""}`}
            onClick={() => setLocale("zh" as Locale)}
          >
            中
          </button>
        </span>
      </div>
    </div>
  );
}
