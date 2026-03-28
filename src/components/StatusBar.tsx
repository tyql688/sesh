import { onMount } from "solid-js";
import { useI18n } from "../i18n/index";
import type { Locale } from "../i18n/index";
import { theme, setTheme, applyTheme } from "../stores/theme";
import type { Theme } from "../stores/theme";

export function StatusBar(props: { sessionCount: number; providerCount: number; isIndexing?: boolean }) {
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
      case "light": return "☀️";
      case "dark": return "🌙";
      case "system": return "💻";
    }
  };

  const themeLabel = () => {
    switch (theme()) {
      case "light": return t("status.themeLight");
      case "dark": return t("status.themeDark");
      case "system": return t("status.themeSystem");
    }
  };

  return (
    <div class="statusbar">
      <div class="statusbar-left">
        <span class={props.isIndexing ? "status-dot-indexing" : "status-dot"} />
        <span>
          {props.isIndexing
            ? t("status.indexing")
            : <>{t("status.indexed")} — {props.sessionCount.toLocaleString()} {t("status.sessions")}</>
          }
        </span>
        <span class="status-separator">·</span>
        <span>
          {props.providerCount} {t("status.providers")}
        </span>
      </div>
      <div class="statusbar-right">
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
