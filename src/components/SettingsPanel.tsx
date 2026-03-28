import { createSignal, createResource, onMount, For, Show } from "solid-js";
import { useI18n, setLocale } from "../i18n/index";
import type { Locale } from "../i18n/index";
import type { IndexStats, ProviderInfo } from "../lib/types";
import { getIndexStats, rebuildIndex, clearIndex, getProviderPaths } from "../lib/tauri";
import { toast, toastError } from "../stores/toast";
import { theme, setTheme } from "../stores/theme";
import type { Theme } from "../stores/theme";
import { terminalApp, setTerminalApp, disabledProviders, toggleProvider, timeGrouping, setTimeGrouping } from "../stores/settings";
import type { TerminalApp } from "../stores/settings";

type SettingsCategory = "general" | "dataSources" | "index" | "keyboard" | "about";

const isMac = navigator.platform.includes("Mac");

export function SettingsPanel() {
  const { t, locale } = useI18n();
  const [activeCategory, setActiveCategory] = createSignal<SettingsCategory>("general");
  const [isRebuilding, setIsRebuilding] = createSignal(false);
  const [version, setVersion] = createSignal("0.1.0");

  onMount(async () => {
    try {
      const { getVersion } = await import("@tauri-apps/api/app");
      setVersion(await getVersion());
    } catch { /* fallback */ }
  });

  const [indexStats, { refetch: refetchStats }] = createResource<IndexStats>(async () => {
    try {
      return await getIndexStats();
    } catch {
      return { session_count: 0, db_size_bytes: 0, last_index_time: "" };
    }
  });

  const [providerPaths, { refetch: refetchProviderPaths }] = createResource<ProviderInfo[]>(async () => {
    try {
      return await getProviderPaths();
    } catch {
      return [];
    }
  });

  const categories = [
    { id: "general" as SettingsCategory, labelKey: "settings.general" as const },
    { id: "dataSources" as SettingsCategory, labelKey: "settings.dataSources" as const },
    { id: "index" as SettingsCategory, labelKey: "settings.index" as const },
    { id: "keyboard" as SettingsCategory, labelKey: "keyboard.title" as const },
    { id: "about" as SettingsCategory, labelKey: "settings.about" as const },
  ];

  const validThemes: Theme[] = ["light", "dark", "system"];
  const isMac = navigator.platform.includes("Mac");
  const validTerminals: TerminalApp[] = isMac
    ? ["terminal", "iterm2", "ghostty", "kitty", "warp", "wezterm", "alacritty"]
    : ["windows-terminal", "powershell", "cmd"];

  function handleThemeChange(value: string) {
    if (validThemes.includes(value as Theme)) setTheme(value as Theme);
  }

  function handleLanguageChange(value: string) {
    setLocale(value as Locale);
  }

  function handleTerminalChange(value: string) {
    if (validTerminals.includes(value as TerminalApp)) setTerminalApp(value as TerminalApp);
  }

  async function handleRebuildIndex() {
    setIsRebuilding(true);
    try {
      await rebuildIndex();
      refetchStats();
      refetchProviderPaths();
      toast(t("toast.rebuildOk"));
    } catch (e) {
      toastError(t("toast.rebuildFailed"));
    } finally {
      setIsRebuilding(false);
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  return (
    <div class="settings-panel">
      <div class="settings-sidebar">
        <For each={categories}>
          {(cat) => (
            <button
              class={`settings-nav-item${activeCategory() === cat.id ? " active" : ""}`}
              onClick={() => setActiveCategory(cat.id)}
            >
              {t(cat.labelKey)}
            </button>
          )}
        </For>
      </div>

      <div class="settings-content">
        <Show when={activeCategory() === "general"}>
          <div class="settings-section">
            <div class="settings-section-title">{t("settings.general")}</div>

            <div class="settings-row">
              <div>
                <div class="settings-label">{t("settings.theme")}</div>
              </div>
              <select
                class="settings-select"
                value={theme()}
                onChange={(e) => handleThemeChange(e.currentTarget.value)}
              >
                <option value="light">{t("settings.themeLight")}</option>
                <option value="dark">{t("settings.themeDark")}</option>
                <option value="system">{t("settings.themeSystem")}</option>
              </select>
            </div>

            <div class="settings-row">
              <div>
                <div class="settings-label">{t("settings.language")}</div>
              </div>
              <select
                class="settings-select"
                value={locale()}
                onChange={(e) => handleLanguageChange(e.currentTarget.value)}
              >
                <option value="en">{t("settings.languageEnglish")}</option>
                <option value="zh">{t("settings.languageChinese")}</option>
              </select>
            </div>

            <div class="settings-row">
              <div>
                <div class="settings-label">{t("settings.terminal")}</div>
                <div class="settings-desc">{t("settings.terminalDesc")}</div>
              </div>
              <select
                class="settings-select"
                value={terminalApp()}
                onChange={(e) => handleTerminalChange(e.currentTarget.value)}
              >
                {isMac ? (<>
                  <option value="terminal">Terminal.app</option>
                  <option value="iterm2">iTerm2</option>
                  <option value="ghostty">Ghostty</option>
                  <option value="kitty">Kitty</option>
                  <option value="warp">Warp</option>
                  <option value="wezterm">WezTerm</option>
                  <option value="alacritty">Alacritty</option>
                </>) : (<>
                  <option value="windows-terminal">Windows Terminal</option>
                  <option value="powershell">PowerShell</option>
                  <option value="cmd">Command Prompt</option>
                </>)}
              </select>
            </div>

            <div class="settings-row">
              <div>
                <div class="settings-label">{t("settings.timeGrouping")}</div>
                <div class="settings-desc">{t("settings.timeGroupingDesc")}</div>
              </div>
              <input
                type="checkbox"
                class="settings-checkbox"
                checked={timeGrouping()}
                onChange={(e) => setTimeGrouping(e.currentTarget.checked)}
              />
            </div>
          </div>
        </Show>

        <Show when={activeCategory() === "dataSources"}>
          <div class="settings-section">
            <div class="settings-section-title">{t("settings.dataSources")}</div>
            <Show when={providerPaths()}>
              <For each={providerPaths()}>
                {(info) => (
                  <div class="settings-row">
                    <div>
                      <div class="settings-label">{info.label}</div>
                      <div class="settings-desc flex-center-gap-sm">
                        <span>{info.path}</span>
                        <Show when={info.exists}>
                          <button
                            class="settings-open-folder"
                            title={t("settings.openInFinder")}
                            onClick={async () => {
                              try {
                                const { open } = await import("@tauri-apps/plugin-shell");
                                await open(info.path);
                              } catch (e) {
                                console.warn("failed to open folder:", info.path, e);
                              }
                            }}
                          >
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                              <path d="M18 13v6a2 2 0 01-2 2H5a2 2 0 01-2-2V8a2 2 0 012-2h6" />
                              <polyline points="15 3 21 3 21 9" />
                              <line x1="10" y1="14" x2="21" y2="3" />
                            </svg>
                          </button>
                        </Show>
                      </div>
                    </div>
                    <div class="flex-center-gap-md">
                      <span class="settings-stat">
                        {info.session_count} {t("status.sessions")}
                      </span>
                      <Show when={info.exists}>
                        <button
                          class={`settings-btn${disabledProviders().includes(info.key) ? " settings-btn-danger" : ""}`}
                          onClick={() => toggleProvider(info.key)}
                        >
                          {disabledProviders().includes(info.key)
                            ? t("settings.disabled")
                            : t("settings.enabled")}
                        </button>
                      </Show>
                      <Show when={!info.exists}>
                        <span class="settings-stat text-danger">
                          {t("settings.disabled")}
                        </span>
                      </Show>
                    </div>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </Show>

        <Show when={activeCategory() === "index"}>
          <div class="settings-section">
            <div class="settings-section-title">{t("settings.index")}</div>

            <div class="settings-row">
              <div class="settings-label">{t("settings.totalSessions")}</div>
              <span class="settings-stat">{indexStats()?.session_count ?? 0}</span>
            </div>

            <div class="settings-row">
              <div class="settings-label">{t("settings.dbSize")}</div>
              <span class="settings-stat">{formatBytes(indexStats()?.db_size_bytes ?? 0)}</span>
            </div>

            <div class="settings-row settings-row-spaced">
              <button
                class="settings-btn"
                onClick={handleRebuildIndex}
                disabled={isRebuilding()}
              >
                {isRebuilding() ? "..." : t("settings.rebuildIndex")}
              </button>
              <button
                class="settings-btn settings-btn-danger"
                onClick={async () => {
                  if (!confirm(t("settings.clearIndexConfirm"))) return;
                  try {
                    await clearIndex();
                    toast(t("toast.clearIndexOk"));
                    refetchStats();
                    refetchProviderPaths();
                  } catch (e) {
                    toastError(String(e));
                  }
                }}
              >
                {t("settings.clearIndex")}
              </button>
            </div>
          </div>
        </Show>

        <Show when={activeCategory() === "keyboard"}>
          <div class="settings-section">
            <div class="settings-section-title">{t("keyboard.title")}</div>

            <div class="settings-shortcuts-group">
              <div class="settings-shortcuts-label">{t("keyboard.navigation")}</div>
              <div class="settings-shortcut-row"><span>{t("keyboard.search")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}K</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.switchTab")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}1-9</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.nextTab")}</span><kbd>{isMac ? "\u2318]" : "Ctrl+Tab"}</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.prevTab")}</span><kbd>{isMac ? "\u2318[" : "Shift+Ctrl+Tab"}</kbd></div>
            </div>

            <div class="settings-shortcuts-group">
              <div class="settings-shortcuts-label">{t("keyboard.tabs")}</div>
              <div class="settings-shortcut-row"><span>{t("keyboard.closeTab")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}W</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.closeAllTabs")}</span><kbd>{isMac ? "\u21E7\u2318" : "Shift+Ctrl+"}W</kbd></div>
            </div>

            <div class="settings-shortcuts-group">
              <div class="settings-shortcuts-label">{t("keyboard.session")}</div>
              <div class="settings-shortcut-row"><span>{t("keyboard.resumeSession")}</span><kbd>{isMac ? "\u21E7\u2318" : "Shift+Ctrl+"}R</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.exportSession")}</span><kbd>{isMac ? "\u21E7\u2318" : "Shift+Ctrl+"}E</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.toggleFavorite")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}B</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.toggleWatch")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}L</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.deleteSession")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}{"\u232B"}</kbd></div>
            </div>

            <div class="settings-shortcuts-group">
              <div class="settings-shortcuts-label">{t("keyboard.general")}</div>
              <div class="settings-shortcut-row"><span>{t("keyboard.showShortcuts")}</span><kbd>{isMac ? "\u2318" : "Ctrl+"}/</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.showShortcuts")}</span><kbd>?</kbd></div>
              <div class="settings-shortcut-row"><span>{t("keyboard.escape")}</span><kbd>Esc</kbd></div>
            </div>
          </div>
        </Show>

        <Show when={activeCategory() === "about"}>
          <div class="settings-section">
            <div class="settings-section-title">{t("settings.about")}</div>

            <div class="settings-row">
              <div class="settings-label">{t("settings.version")}</div>
              <span class="settings-stat">{version()}</span>
            </div>

            <div class="settings-row">
              <div class="settings-label">{t("settings.github")}</div>
              <a
                class="settings-stat link-accent"
                href="https://github.com/tyql688/cc-session"
                target="_blank"
                rel="noopener noreferrer"
              >
                cc-session
              </a>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
